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
use tokio::io::{copy as async_copy, BufReader as AsyncBufReader, BufWriter as AsyncBufWriter};
use tokio::sync::oneshot::channel;
use tokio::sync::oneshot::error::RecvError;
use tokio::task::{JoinError, JoinSet};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::io::InspectReader;

use crate::debian::{
    ArchitectureName, Distro, PackageIndex, ParseRepositoryPackageError, RepositoryPackage,
    RepositoryUri, Source,
};
use crate::pgp::CertHelper;
use crate::{BuildpackResult, DebianPackagesBuildpack, DebianPackagesBuildpackError};

pub(crate) async fn create_package_index(
    context: &Arc<BuildContext<DebianPackagesBuildpack>>,
    client: &ClientWithMiddleware,
    distro: &Distro,
) -> BuildpackResult<PackageIndex> {
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
) -> BuildpackResult<Vec<(RepositoryUri, PathBuf)>> {
    if sources.is_empty() {
        Err(CreatePackageIndexError::NoSources)?;
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
        for updated_source in
            update_source_handle.map_err(CreatePackageIndexError::TaskFailed)??
        {
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
) -> BuildpackResult<Vec<(RepositoryUri, PathBuf)>> {
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
        let package_index = format!("{component}/binary-{arch}/Packages.gz");
        let package_index_release_hash = release
            .sha256sum
            .as_ref()
            .ok_or(CreatePackageIndexError::MissingSha256ReleaseHashes(
                uri.clone(),
            ))?
            .iter()
            .find(|release_hash| release_hash.filename == package_index)
            .ok_or(CreatePackageIndexError::MissingPackageIndexReleaseHash(
                uri.clone(),
                package_index,
            ))?;

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
        ));
    }

    let mut results = vec![];
    while let Some(get_package_list_handle) = get_package_list_handles.join_next().await {
        results.push((
            uri.clone(),
            get_package_list_handle.map_err(CreatePackageIndexError::TaskFailed)??,
        ));
    }
    Ok(results)
}

async fn get_release(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    uri: RepositoryUri,
    suite: String,
    signed_by: String,
) -> BuildpackResult<Release> {
    let release_url = format!("{}/dists/{suite}/InRelease", uri.as_str());

    let response = client
        .get(&release_url)
        .send()
        .await
        .and_then(|res| res.error_for_status().map_err(Reqwest))
        .map_err(CreatePackageIndexError::GetReleaseRequest)?;

    // it would be nice to use the url as the layer name but urls don't make for good file names
    // so instead we'll convert the url to a sha256 hex value
    let layer_name = LayerName::from_str(&format!("{:x}", Sha256::digest(&release_url)))
        .map_err(|e| CreatePackageIndexError::InvalidLayerName(release_url.clone(), e))?;

    let new_metadata = ReleaseFileMetadata {
        etag: response.headers().get(ETAG).and_then(|header_value| {
            if let Ok(etag) = header_value.to_str() {
                Some(etag.to_string())
            } else {
                None
            }
        }),
    };

    let release_file_layer = context.cached_layer(
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
    )?;

    let release_file_path = release_file_layer.path().join("release");

    match release_file_layer.state {
        LayerState::Restored { .. } => {
            println!("  [CACHED] {url}", url = &release_url);
        }
        LayerState::Empty { .. } => {
            release_file_layer.write_metadata(new_metadata)?;

            let raw_release_url_path = release_file_layer.path().join(".url");
            async_write(&raw_release_url_path, &release_url)
                .await
                .map_err(|e| CreatePackageIndexError::WriteReleaseLayer(raw_release_url_path, e))?;

            println!("  [GET] {url}", url = &release_url);

            let unverified_response_body = response
                .text()
                .await
                .map_err(CreatePackageIndexError::ReadGetReleaseResponse)?;

            // GPG verification
            let policy = StandardPolicy::new();
            let cert_helper = Cert::from_str(&signed_by)
                .map_err(CreatePackageIndexError::CreatePgpCertificate)
                .map(CertHelper::new)?;

            let mut reader = FuturesAsyncReadCompatExt::compat(AllowStdIo::new(
                VerifierBuilder::from_bytes(&unverified_response_body)
                    .map_err(CreatePackageIndexError::CreatePgpVerifier)
                    .and_then(|verifier_builder| {
                        verifier_builder
                            .with_policy(&policy, None, cert_helper)
                            .map_err(CreatePackageIndexError::CreatePgpVerifier)
                    })?,
            ));

            let mut writer = AsyncFile::create(&release_file_path)
                .await
                .map_err(|e| {
                    CreatePackageIndexError::WriteReleaseLayer(release_file_path.clone(), e)
                })
                .map(AsyncBufWriter::new)?;

            async_copy(&mut reader, &mut writer).await.map_err(|e| {
                CreatePackageIndexError::WriteReleaseLayer(release_file_path.clone(), e)
            })?;
        }
    };

    Ok(async_read_to_string(&release_file_path)
        .await
        .map_err(|e| CreatePackageIndexError::ReadReleaseFile(release_file_path.clone(), e))
        .and_then(|release_data| {
            Release::from(&release_data)
                .map_err(|e| CreatePackageIndexError::ParseReleaseFile(release_file_path, e))
        })?)
}

async fn get_package_list(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    package_release_url: String,
    hash: String,
) -> BuildpackResult<PathBuf> {
    // it would be nice to use the url as the layer name but urls don't make for good file names
    // so instead we'll convert the url to a sha256 hex value
    let layer_name = LayerName::from_str(&format!("{:x}", Sha256::digest(&package_release_url)))
        .map_err(|e| CreatePackageIndexError::InvalidLayerName(package_release_url.clone(), e))?;

    let new_metadata = PackageIndexMetadata {
        hash: hash.to_string(),
    };

    let package_index_layer = context.cached_layer(
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
    )?;

    let package_index_path = package_index_layer.path().join("package_index");

    match package_index_layer.state {
        LayerState::Restored { .. } => {
            println!("  [CACHED] {url}", url = &package_release_url);
        }
        LayerState::Empty { .. } => {
            println!("  [GET] {url}", url = &package_release_url);

            package_index_layer.write_metadata(new_metadata)?;

            let package_index_url_path = package_index_layer.path().join(".url");
            async_write(&package_index_url_path, &package_release_url)
                .await
                .map_err(|e| {
                    CreatePackageIndexError::WritePackagesLayer(package_index_url_path, e)
                })?;

            let response = client
                .get(&package_release_url)
                .send()
                .await
                .and_then(|res| res.error_for_status().map_err(Reqwest))
                .map_err(CreatePackageIndexError::GetPackagesRequest)?;

            let mut hasher = Sha256::new();

            // the package list we request uses gzip compression so we'll decode that directly from the response
            let mut reader = GzipDecoder::new(AsyncBufReader::new(
                // the inspect reader lets us pipe this decompressed output to both the ouptut file and the hash digest
                InspectReader::new(
                    // and we need to convert the http stream into an async reader
                    FuturesAsyncReadCompatExt::compat(
                        response
                            .bytes_stream()
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                            .into_async_read(),
                    ),
                    |bytes| hasher.update(bytes),
                ),
            ));

            let mut writer = AsyncFile::create(&package_index_path)
                .await
                .map_err(|e| {
                    CreatePackageIndexError::WritePackagesLayer(package_index_path.clone(), e)
                })
                .map(AsyncBufWriter::new)?;

            async_copy(&mut reader, &mut writer).await.map_err(|e| {
                CreatePackageIndexError::WritePackageIndexFromResponse(
                    package_index_path.clone(),
                    e,
                )
            })?;

            let calculated_hash = format!("{:x}", hasher.finalize());

            if hash != calculated_hash {
                Err(CreatePackageIndexError::ChecksumFailed {
                    url: package_release_url,
                    expected: hash,
                    actual: calculated_hash,
                })?;
            }
        }
    };

    Ok(package_index_path)
}

async fn build_package_index(
    updated_sources: Vec<(RepositoryUri, PathBuf)>,
) -> BuildpackResult<PackageIndex> {
    let mut get_packages_handles = JoinSet::new();
    for (repository, package_index_path) in updated_sources {
        get_packages_handles.spawn(read_packages(repository, package_index_path));
    }

    let mut package_index = PackageIndex::default();
    while let Some(get_package_handle) = get_packages_handles.join_next().await {
        let packages = get_package_handle.map_err(CreatePackageIndexError::TaskFailed)??;
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
) -> BuildpackResult<Vec<RepositoryPackage>> {
    let contents = async_read_to_string(&package_index_path)
        .await
        .map_err(|e| CreatePackageIndexError::ReadPackagesFile(package_index_path.clone(), e))?
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
    let (packages, errors) = recv.await.map_err(CreatePackageIndexError::CpuTaskFailed)?;
    if errors.is_empty() {
        Ok(packages)
    } else {
        Err(CreatePackageIndexError::ParsePackages(package_index_path, errors).into())
    }
}

#[derive(Debug)]
pub(crate) enum CreatePackageIndexError {
    NoSources,
    TaskFailed(JoinError),
    InvalidLayerName(String, LayerNameError),
    GetReleaseRequest(reqwest_middleware::Error),
    ReadGetReleaseResponse(reqwest::Error),
    CreatePgpCertificate(anyhow::Error),
    CreatePgpVerifier(anyhow::Error),
    WriteReleaseLayer(PathBuf, std::io::Error),
    ReadReleaseFile(PathBuf, std::io::Error),
    ParseReleaseFile(PathBuf, APTError),
    MissingSha256ReleaseHashes(RepositoryUri),
    MissingPackageIndexReleaseHash(RepositoryUri, String),
    GetPackagesRequest(reqwest_middleware::Error),
    WritePackagesLayer(PathBuf, std::io::Error),
    WritePackageIndexFromResponse(PathBuf, std::io::Error),
    ChecksumFailed {
        url: String,
        expected: String,
        actual: String,
    },
    CpuTaskFailed(RecvError),
    ReadPackagesFile(PathBuf, std::io::Error),
    ParsePackages(PathBuf, Vec<ParseRepositoryPackageError>),
}

impl From<CreatePackageIndexError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: CreatePackageIndexError) -> Self {
        Self::BuildpackError(DebianPackagesBuildpackError::CreatePackageIndex(value))
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct PackageIndexMetadata {
    hash: String,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ReleaseFileMetadata {
    etag: Option<String>,
}
