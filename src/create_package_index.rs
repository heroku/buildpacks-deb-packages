use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use apt_parser::errors::APTError;
use apt_parser::Release;
use async_compression::tokio::bufread::GzipDecoder;
use futures::io::AllowStdIo;
use futures::TryStreamExt;
use libcnb::build::BuildContext;
use libcnb::data::layer::{LayerName, LayerNameError};
use libcnb::layer::{
    CachedLayerDefinition, InvalidMetadataAction, LayerState, RestoredLayerAction,
};
use rayon::iter::{Either, IntoParallelIterator, ParallelBridge, ParallelIterator};
use reqwest::header::ETAG;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_middleware::Error::Reqwest;
use sequoia_openpgp::parse::stream::VerifierBuilder;
use sequoia_openpgp::parse::Parse;
use sequoia_openpgp::policy::StandardPolicy;
use sequoia_openpgp::Cert;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs::{read_to_string as async_read_to_string, write as async_write, File as AsyncFile};
use tokio::io::{
    copy as async_copy, AsyncWriteExt, BufReader as AsyncBufReader, BufWriter as AsyncBufWriter,
};
use tokio::sync::oneshot::channel;
use tokio::sync::oneshot::error::RecvError;
use tokio::task::{JoinError, JoinSet};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::io::InspectReader;

use crate::create_package_index::CreatePackageIndexError::{
    ChecksumFailed, CpuTaskFailed, CreateLayer, CreatePgpVerifier, GetPackagesRequest,
    GetReleaseRequest, InvalidLayerName, MissingPackageIndexReleaseHash,
    MissingSha256ReleaseHashes, NoSources, ParsePackages, ParseReleaseFile, ReadGetReleaseResponse,
    ReadPackagesFile, ReadPgpCertificate, ReadReleaseFile, TaskFailed, WriteLayerMetadata,
    WritePackagesLayer, WriteReleaseLayer,
};
use crate::debian::{
    ArchitectureName, Distro, PackageIndex, ParseRepositoryPackageError, RepositoryPackage,
    RepositoryUri, Source,
};
use crate::pgp::CertHelper;
use crate::{DebianPackagesBuildpack, DebianPackagesBuildpackError};

type Result<T> = std::result::Result<T, CreatePackageIndexError>;

pub(crate) async fn create_package_index(
    context: &Arc<BuildContext<DebianPackagesBuildpack>>,
    client: &ClientWithMiddleware,
    distro: &Distro,
) -> Result<PackageIndex> {
    println!("## Creating package index");
    println!();
    let updated_sources = update_sources(context, client, &distro.get_source_list()).await?;
    println!();

    println!("  Processing package files...");
    let start = std::time::Instant::now();
    let package_index = build_package_index(updated_sources).await?;
    println!(
        "  Indexed {} packages ({}ms)",
        package_index.packages_indexed,
        start.elapsed().as_millis()
    );
    println!();

    Ok(package_index)
}

async fn update_sources(
    context: &Arc<BuildContext<DebianPackagesBuildpack>>,
    client: &ClientWithMiddleware,
    sources: &[Source],
) -> Result<Vec<(RepositoryUri, PathBuf)>> {
    if sources.is_empty() {
        Err(NoSources)?;
    }

    let mut update_source_handles = JoinSet::new();

    for source in sources {
        for suite in &source.suites {
            update_source_handles.spawn(update_source(
                context.clone(),
                client.clone(),
                source.uri.clone(),
                suite.to_string(),
                source.components.clone(),
                source.arch.clone(),
                source.signed_by.to_string(),
            ));
        }
    }

    let mut updated_sources = vec![];
    while let Some(update_source_handle) = update_source_handles.join_next().await {
        for updated_source in update_source_handle.map_err(TaskFailed)?? {
            updated_sources.push(updated_source);
        }
    }

    Ok(updated_sources)
}

async fn update_source(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    uri: RepositoryUri,
    suite: String,
    components: Vec<String>,
    arch: ArchitectureName,
    signed_by: String,
) -> Result<Vec<(RepositoryUri, PathBuf)>> {
    let release = get_release(
        context.clone(),
        client.clone(),
        uri.clone(),
        suite.clone(),
        signed_by,
    )
    .await?;

    let mut get_package_list_handles = JoinSet::new();

    for component in components {
        let package_index_release_hash = release
            .sha256sum
            .as_ref()
            .ok_or(MissingSha256ReleaseHashes)?
            .iter()
            .find(|release_hash| {
                release_hash.filename == format!("{component}/binary-{arch}/Packages.gz")
            })
            .ok_or(MissingPackageIndexReleaseHash)?;

        let package_release_url = if release.acquire_by_hash.unwrap_or_default() {
            format!(
                "{}/dists/{suite}/{component}/binary-{arch}/by-hash/SHA256/{}",
                uri.as_str(),
                package_index_release_hash.hash
            )
        } else {
            format!(
                "{}/dists/{suite}/{component}/binary-{arch}/Packages.gz",
                uri.as_str()
            )
        };

        get_package_list_handles.spawn(get_package_list(
            context.clone(),
            client.clone(),
            package_release_url,
            package_index_release_hash.hash.to_string(),
            package_index_release_hash.size,
            format!("{}/dists/{suite}/InRelease", uri.as_str()),
        ));
    }

    let mut results = vec![];
    while let Some(get_package_list_handle) = get_package_list_handles.join_next().await {
        results.push((uri.clone(), get_package_list_handle.map_err(TaskFailed)??));
    }
    Ok(results)
}

async fn get_release(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    uri: RepositoryUri,
    suite: String,
    signed_by: String,
) -> Result<Release> {
    let release_url = format!("{}/dists/{suite}/InRelease", uri.as_str());

    let response = client
        .get(&release_url)
        .send()
        .await
        .and_then(|res| res.error_for_status().map_err(Reqwest))
        .map_err(GetReleaseRequest)?;

    // it would be nice to use the url as the layer name but urls don't make for good file names
    // so instead we'll convert the url to a sha256 hex value
    let layer_name = LayerName::from_str(&format!("{:x}", Sha256::digest(&release_url)))
        .map_err(InvalidLayerName)?;

    let new_metadata = ReleaseFileMetadata {
        etag: response.headers().get(ETAG).and_then(|header_value| {
            if let Ok(etag) = header_value.to_str() {
                Some(etag.to_string())
            } else {
                None
            }
        }),
    };

    let release_file_layer = context
        .cached_layer(
            layer_name,
            CachedLayerDefinition {
                build: true,
                launch: false,
                restored_layer_action: &|old_metadata: &ReleaseFileMetadata, _| {
                    if old_metadata == &new_metadata {
                        RestoredLayerAction::KeepLayer
                    } else {
                        RestoredLayerAction::DeleteLayer
                    }
                },
                invalid_metadata_action: &|_| InvalidMetadataAction::DeleteLayer,
            },
        )
        .map_err(|e| CreateLayer(Box::new(e)))?;

    let release_file_path = release_file_layer.path().join("release");

    match release_file_layer.state {
        LayerState::Restored { .. } => {
            println!("  [CACHED] {url}", url = &release_url);
        }
        LayerState::Empty { .. } => {
            release_file_layer
                .write_metadata(new_metadata)
                .map_err(|e| WriteLayerMetadata(Box::new(e)))?;

            async_write(release_file_layer.path().join(".url"), &release_url)
                .await
                .map_err(WriteReleaseLayer)?;

            println!("  [GET] {url}", url = &release_url);

            let unverified_response_body = response.text().await.map_err(ReadGetReleaseResponse)?;

            // GPG verification
            let policy = StandardPolicy::new();
            let cert_helper = Cert::from_str(&signed_by)
                .map_err(ReadPgpCertificate)
                .map(CertHelper::new)?;

            let mut reader = FuturesAsyncReadCompatExt::compat(AllowStdIo::new(
                VerifierBuilder::from_bytes(&unverified_response_body)
                    .map_err(CreatePgpVerifier)
                    .and_then(|verifier_builder| {
                        verifier_builder
                            .with_policy(&policy, None, cert_helper)
                            .map_err(CreatePgpVerifier)
                    })?,
            ));

            let mut writer = AsyncFile::create(&release_file_path)
                .await
                .map_err(WriteReleaseLayer)
                .map(AsyncBufWriter::new)?;

            async_copy(&mut reader, &mut writer)
                .await
                .map_err(WriteReleaseLayer)?;
        }
    };

    async_read_to_string(&release_file_path)
        .await
        .map_err(ReadReleaseFile)
        .and_then(|release_data| Release::from(&release_data).map_err(ParseReleaseFile))
}

async fn get_package_list(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    package_release_url: String,
    hash: String,
    size: u64,
    release_url: String,
) -> Result<PathBuf> {
    // it would be nice to use the url as the layer name but urls don't make for good file names
    // so instead we'll convert the url to a sha256 hex value
    let layer_name = LayerName::from_str(&format!("{:x}", Sha256::digest(&package_release_url)))
        .map_err(InvalidLayerName)?;

    let new_metadata = PackageIndexMetadata {
        hash: hash.to_string(),
    };

    let package_index_layer = context
        .cached_layer(
            layer_name,
            CachedLayerDefinition {
                build: true,
                launch: false,
                restored_layer_action: &|old_metadata: &PackageIndexMetadata, _| {
                    if old_metadata == &new_metadata {
                        RestoredLayerAction::KeepLayer
                    } else {
                        RestoredLayerAction::DeleteLayer
                    }
                },
                invalid_metadata_action: &|_| InvalidMetadataAction::DeleteLayer,
            },
        )
        .map_err(|e| CreateLayer(Box::new(e)))?;

    let package_index_path = package_index_layer.path().join("package_index");

    match package_index_layer.state {
        LayerState::Restored { .. } => {
            println!("  [CACHED] {url}", url = &package_release_url);
        }
        LayerState::Empty { .. } => {
            println!("  [GET] {url}", url = &package_release_url);

            package_index_layer
                .write_metadata(new_metadata)
                .map_err(|e| WriteLayerMetadata(Box::new(e)))?;

            async_write(
                package_index_layer.path().join(".url"),
                &package_release_url,
            )
            .await
            .map_err(WritePackagesLayer)?;

            let response = client
                .get(&package_release_url)
                .send()
                .await
                .and_then(|res| res.error_for_status().map_err(Reqwest))
                .map_err(GetPackagesRequest)?;

            let mut hasher = Sha256::new();
            let mut bytes_seen: usize = 0;
            // the package list we request uses gzip compression so we'll decode that directly from the response
            let mut reader = GzipDecoder::new(AsyncBufReader::new(
                // the inspect reader lets us pipe this decompressed output to both the output file and the hash digest
                InspectReader::new(
                    // and we need to convert the http stream into an async reader
                    FuturesAsyncReadCompatExt::compat(
                        response
                            .bytes_stream()
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                            .into_async_read(),
                    ),
                    |bytes| {
                        bytes_seen += bytes.len();
                        hasher.update(bytes);
                    },
                ),
            ));

            // Enable support for multistream gz files. In this mode, the reader expects the input to
            // be a sequence of individually gzipped data streams, each with its own header and trailer,
            // ending at EOF. This is standard behavior for gzip readers.
            reader.multiple_members(true);

            let mut writer = AsyncFile::create(&package_index_path)
                .await
                .map_err(WritePackagesLayer)
                .map(AsyncBufWriter::new)?;

            async_copy(&mut reader, &mut writer)
                .await
                .map_err(WritePackagesLayer)?;

            // writer.flush().await.unwrap();

            let calculated_hash = format!("{:x}", hasher.finalize());

            if hash != calculated_hash {
                let cmd_sha = std::process::Command::new("sha256sum")
                    .arg(&package_index_path)
                    .output()
                    .map(|output| {
                        if output.status.success() {
                            String::from_utf8(output.stdout)
                                .unwrap()
                                .split(' ')
                                .next()
                                .unwrap_or("!sha256sum split failed")
                                .to_string()
                        } else {
                            "!sha256sum output failed".to_string()
                        }
                    })
                    .unwrap_or("!sha256sum command failed".to_string());
                let file_size = package_index_path
                    .metadata()
                    .map(|m| m.len().to_string())
                    .unwrap_or("!file size error".to_string());
                println!(
                    "
***
release_url: {release_url}
url: {package_release_url}
uncompressed size: {file_size}
compressed size:   {bytes_seen}
expected size:     {size}
sha256sum hash:  {cmd_sha}
calculated hash: {calculated_hash}
expected hash:   {hash}
***
            "
                );
                Err(ChecksumFailed(package_release_url, hash, calculated_hash))?;
            }
        }
    };

    Ok(package_index_path)
}

async fn build_package_index(
    updated_sources: Vec<(RepositoryUri, PathBuf)>,
) -> Result<PackageIndex> {
    let mut get_packages_handles = JoinSet::new();
    for (repository, package_index_path) in updated_sources {
        get_packages_handles.spawn(read_packages(repository, package_index_path));
    }

    let mut package_index = PackageIndex::default();
    while let Some(get_package_handle) = get_packages_handles.join_next().await {
        let packages = get_package_handle.map_err(TaskFailed)??;
        for package in packages {
            package_index.add_package(package);
        }
    }

    Ok(package_index)
}

// NOTE: Rayon is used here since this is a fairly CPU-intensive operation.
//       See - https://ryhl.io/blog/async-what-is-blocking/
async fn read_packages(
    repository_uri: RepositoryUri,
    package_index_path: PathBuf,
) -> Result<Vec<RepositoryPackage>> {
    let contents = async_read_to_string(&package_index_path)
        .await
        .map_err(ReadPackagesFile)?
        .replace("\r\n", "\n")
        .replace('\0', "");

    let (send, recv) = channel();
    rayon::spawn(move || {
        let (errors, packages): (Vec<_>, Vec<_>) = contents
            .trim()
            .split("\n\n")
            .par_bridge()
            .into_par_iter()
            .partition_map(|package_data| {
                RepositoryPackage::parse_parallel(repository_uri.clone(), package_data)
                    .map_or_else(Either::Left, Either::Right)
            });
        let _ = send.send((packages, errors));
    });
    let (packages, errors) = recv.await.map_err(CpuTaskFailed)?;
    if errors.is_empty() {
        Ok(packages)
    } else {
        Err(ParsePackages(errors))
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum CreatePackageIndexError {
    NoSources,
    TaskFailed(JoinError),
    InvalidLayerName(LayerNameError),
    CreateLayer(Box<libcnb::Error<DebianPackagesBuildpackError>>),
    WriteLayerMetadata(Box<libcnb::Error<DebianPackagesBuildpackError>>),
    GetReleaseRequest(reqwest_middleware::Error),
    ReadGetReleaseResponse(reqwest::Error),
    ReadPgpCertificate(anyhow::Error),
    CreatePgpVerifier(anyhow::Error),
    WriteReleaseLayer(std::io::Error),
    ReadReleaseFile(std::io::Error),
    ParseReleaseFile(APTError),
    MissingSha256ReleaseHashes,
    MissingPackageIndexReleaseHash,
    GetPackagesRequest(reqwest_middleware::Error),
    WritePackagesLayer(std::io::Error),
    ChecksumFailed(String, String, String),
    CpuTaskFailed(RecvError),
    ReadPackagesFile(std::io::Error),
    ParsePackages(Vec<ParseRepositoryPackageError>),
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct PackageIndexMetadata {
    hash: String,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ReleaseFileMetadata {
    etag: Option<String>,
}
