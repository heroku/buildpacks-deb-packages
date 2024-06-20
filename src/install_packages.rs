use std::env::temp_dir;
use std::ffi::OsString;
use std::fs::File;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use ar::Archive as ArArchive;
use async_compression::tokio::bufread::{GzipDecoder, ZstdDecoder};
use futures::io::AllowStdIo;
use futures::TryStreamExt;
use libcnb::build::BuildContext;
use libcnb::data::layer::{LayerName, LayerNameError};
use libcnb::layer::{
    CachedLayerDefinition, InvalidMetadataAction, LayerState, RestoredLayerAction,
};
use libcnb::layer_env::LayerEnv;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_middleware::Error::Reqwest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs::File as AsyncFile;
use tokio::io::{copy as async_copy, BufReader as AsyncBufReader, BufWriter as AsyncBufWriter};
use tokio::task::{spawn_blocking, JoinError, JoinSet};
use tokio_tar::Archive as TarArchive;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::io::InspectReader;

use crate::debian::{RepositoryPackage, SupportedDistro};
use crate::install_packages::InstallPackagesError::{
    ChecksumFailed, CreateLayer, DownloadPackage, InstallPackage, InvalidLayerName, NoFilename,
    OpenPackageArchive, OpenPackageArchiveEntry, TaskFailed, UnpackTarball, UnsupportedCompression,
    WriteLayerEnv, WriteLayerMetadata, WritePackage,
};
use crate::on_package_install::{on_package_install, OnPackageInstallError};
use crate::{DebianPackagesBuildpack, DebianPackagesBuildpackError};

type Result<T> = std::result::Result<T, InstallPackagesError>;

pub(crate) async fn install_packages(
    context: &Arc<BuildContext<DebianPackagesBuildpack>>,
    client: &ClientWithMiddleware,
    distro: &SupportedDistro,
    packages_to_install: Vec<RepositoryPackage>,
) -> Result<()> {
    println!("## Installing packages");
    println!();

    let mut download_and_extract_handles = JoinSet::new();

    for repository_package in packages_to_install {
        download_and_extract_handles.spawn(download_extract_and_install(
            context.clone(),
            client.clone(),
            distro.clone(),
            repository_package.clone(),
        ));
    }

    while let Some(download_and_extract_handle) = download_and_extract_handles.join_next().await {
        download_and_extract_handle.map_err(TaskFailed)??;
    }

    println!();
    Ok(())
}

async fn download_extract_and_install(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    distro: SupportedDistro,
    repository_package: RepositoryPackage,
) -> Result<()> {
    let new_metadata = ExtractedPackageMetadata {
        hash: repository_package.sha256sum.to_string(),
    };

    let package_name = &repository_package.name;

    let layer_name = LayerName::from_str(package_name).map_err(InvalidLayerName)?;

    let extracted_package_layer = context
        .cached_layer(
            layer_name,
            CachedLayerDefinition {
                launch: true,
                build: true,
                invalid_metadata_action: &|_| InvalidMetadataAction::DeleteLayer,
                restored_layer_action: &|old_metadata: &ExtractedPackageMetadata, _| {
                    if old_metadata == &new_metadata {
                        RestoredLayerAction::KeepLayer
                    } else {
                        RestoredLayerAction::DeleteLayer
                    }
                },
            },
        )
        .map_err(|e| CreateLayer(Box::new(e)))?;

    match extracted_package_layer.state {
        LayerState::Restored { .. } => {
            println!("  Restoring {package_name} from cache");
        }
        LayerState::Empty { .. } => {
            extracted_package_layer
                .write_metadata(new_metadata)
                .map_err(|e| WriteLayerMetadata(Box::new(e)))?;

            println!("  Downloading {package_name}");
            let download_path = download(client, &repository_package).await?;

            println!("  Extracting {package_name}");
            extract(download_path, extracted_package_layer.path()).await?;

            println!(
                "  Installing {package_name} → {}",
                extracted_package_layer.path().display()
            );
            let package_env = install(
                &repository_package,
                &extracted_package_layer.path(),
                &distro,
            )
            .await?;

            extracted_package_layer
                .write_env(package_env)
                .map_err(|e| WriteLayerEnv(Box::new(e)))?;
        }
    }

    Ok(())
}

async fn download(
    client: ClientWithMiddleware,
    repository_package: &RepositoryPackage,
) -> Result<PathBuf> {
    let download_url = format!(
        "{}/{}",
        repository_package.repository_uri.as_str(),
        repository_package.filename.as_str()
    );

    let download_file_name = PathBuf::from(repository_package.filename.as_str())
        .file_name()
        .map(ToOwned::to_owned)
        .ok_or(NoFilename)?;

    let download_path = temp_dir().join::<&Path>(download_file_name.as_ref());

    let response = client
        .get(&download_url)
        .send()
        .await
        .and_then(|res| res.error_for_status().map_err(Reqwest))
        .map_err(DownloadPackage)?;

    let mut hasher = Sha256::new();

    let mut writer = AsyncFile::create(&download_path)
        .await
        .map_err(WritePackage)
        .map(AsyncBufWriter::new)?;

    // the inspect reader lets us pipe the response to both the output file and the hash digest
    let mut reader = AsyncBufReader::new(InspectReader::new(
        // and we need to convert the http stream into an async reader
        FuturesAsyncReadCompatExt::compat(
            response
                .bytes_stream()
                .map_err(|e| std::io::Error::new(ErrorKind::Other, e))
                .into_async_read(),
        ),
        |bytes| hasher.update(bytes),
    ));

    async_copy(&mut reader, &mut writer)
        .await
        .map_err(WritePackage)?;

    let calculated_hash = format!("{:x}", hasher.finalize());

    if repository_package.sha256sum != calculated_hash {
        Err(ChecksumFailed(
            download_url,
            repository_package.sha256sum.to_string(),
            calculated_hash,
        ))?;
    }

    Ok(download_path)
}

async fn extract(download_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    // a .deb file is an ar archive
    // https://manpages.ubuntu.com/manpages/jammy/en/man5/deb.5.html
    let mut debian_archive = File::open(download_path)
        .map_err(OpenPackageArchive)
        .map(ArArchive::new)?;

    while let Some(entry) = debian_archive.next_entry() {
        let entry = entry.map_err(OpenPackageArchiveEntry)?;
        let entry_path = PathBuf::from(OsString::from_vec(entry.header().identifier().to_vec()));
        let entry_reader =
            AsyncBufReader::new(FuturesAsyncReadCompatExt::compat(AllowStdIo::new(entry)));

        // https://manpages.ubuntu.com/manpages/noble/en/man5/deb.5.html
        match (
            entry_path.file_stem().and_then(|v| v.to_str()),
            entry_path.extension().and_then(|v| v.to_str()),
        ) {
            (Some("control.tar" | "data.tar"), Some("gz")) => {
                let mut tar_archive = TarArchive::new(GzipDecoder::new(entry_reader));
                tar_archive
                    .unpack(&output_dir)
                    .await
                    .map_err(UnpackTarball)?;
            }
            (Some("control.tar" | "data.tar"), Some("zstd" | "zst")) => {
                let mut tar_archive = TarArchive::new(ZstdDecoder::new(entry_reader));
                tar_archive
                    .unpack(&output_dir)
                    .await
                    .map_err(UnpackTarball)?;
            }
            (Some("control.tar" | "data.tar"), Some(compression)) => {
                Err(UnsupportedCompression(compression.to_string()))?;
            }
            _ => {
                // ignore other potential file entries (e.g.; debian-binary, control.tar)
            }
        };
    }

    Ok(())
}

async fn install(
    repository_package: &RepositoryPackage,
    install_path: &Path,
    supported_distro: &SupportedDistro,
) -> Result<LayerEnv> {
    let repository_package = repository_package.clone();
    let install_path = install_path.to_path_buf();
    let supported_distro = supported_distro.clone();
    spawn_blocking(move || {
        on_package_install(&repository_package, &install_path, &supported_distro)
            .map_err(InstallPackage)
    })
    .await
    .map_err(TaskFailed)?
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum InstallPackagesError {
    TaskFailed(JoinError),
    InvalidLayerName(LayerNameError),
    NoFilename,
    CreateLayer(Box<libcnb::Error<DebianPackagesBuildpackError>>),
    DownloadPackage(reqwest_middleware::Error),
    WritePackage(std::io::Error),
    ChecksumFailed(String, String, String),
    OpenPackageArchive(std::io::Error),
    OpenPackageArchiveEntry(std::io::Error),
    UnpackTarball(std::io::Error),
    WriteLayerMetadata(Box<libcnb::Error<DebianPackagesBuildpackError>>),
    WriteLayerEnv(Box<libcnb::Error<DebianPackagesBuildpackError>>),
    UnsupportedCompression(String),
    InstallPackage(OnPackageInstallError),
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
struct ExtractedPackageMetadata {
    hash: String,
}
