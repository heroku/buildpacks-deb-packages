use crate::debian::{
    ArchitectureName, PackageIndex, ParseRepositoryPackageError, RepositoryPackage, RepositoryUri,
    Source,
};
use crate::o11y::*;
use crate::pgp::CertHelper;
use crate::{BuildpackResult, DebianPackagesBuildpack, DebianPackagesBuildpackError};
use apt_parser::errors::APTError;
use apt_parser::Release;
use async_compression::tokio::bufread::GzipDecoder;
use bullet_stream::state::Bullet;
use bullet_stream::{style, Print};
use futures::io::AllowStdIo;
use futures::TryStreamExt;
use libcnb::build::BuildContext;
use libcnb::data::layer::{LayerName, LayerNameError};
use libcnb::layer::{
    CachedLayerDefinition, EmptyLayerCause, InvalidMetadataAction, LayerState, RestoredLayerAction,
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
use std::fmt::{Display, Formatter};
use std::io::Stdout;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::fs::{read_to_string as async_read_to_string, write as async_write, File as AsyncFile};
use tokio::io::{
    copy as async_copy, AsyncWriteExt, BufReader as AsyncBufReader, BufWriter as AsyncBufWriter,
};
use tokio::sync::oneshot::channel;
use tokio::sync::oneshot::error::RecvError;
use tokio::task::{JoinError, JoinSet};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::io::InspectReader;
use tracing::{info, instrument, Instrument};

#[instrument(skip_all)]
pub(crate) async fn create_package_index(
    context: &Arc<BuildContext<DebianPackagesBuildpack>>,
    client: &ClientWithMiddleware,
    source_list: &[Source],
    log: Print<Bullet<Stdout>>,
) -> BuildpackResult<(PackageIndex, Print<Bullet<Stdout>>)> {
    let log = log.h2("Creating package index");

    let log = source_list
        .iter()
        .fold(log.bullet("Package sources"), |log, source| {
            source.suites.iter().fold(log, |log, suite| {
                log.sub_bullet(format!(
                    "{repository_uri} {suite} [{components}]",
                    repository_uri = style::url(source.uri.as_str()),
                    components = source.components.join(", "),
                ))
            })
        });

    let timer = log.start_timer("Updating");
    let updated_sources = update_sources(context, client, &source_list).await?;
    let log = timer.done();

    let log = updated_sources
        .iter()
        .fold(log, |log, updated_source| {
            let update_source_log =
                log.sub_bullet(match &updated_source.release_file.cache_state {
                    UpdatedSourceCacheState::Cached => format!(
                        "Restored release file from cache {url}",
                        url = style::details(style::url(
                            &updated_source.release_file.release_file_url
                        ))
                    ),
                    UpdatedSourceCacheState::New => format!(
                        "Downloaded release file {url}",
                        url = style::url(&updated_source.release_file.release_file_url)
                    ),
                    UpdatedSourceCacheState::Invalidated(reason) => format!(
                        "Redownloaded release file {url} {reason}",
                        url = style::url(&updated_source.release_file.release_file_url),
                        reason = style::details(reason)
                    ),
                });

            updated_source.package_indexes.iter().fold(
                update_source_log,
                |update_source_log, updated_package_index| {
                    update_source_log.sub_bullet(match &updated_package_index.cache_state {
                        UpdatedSourceCacheState::Cached => format!(
                            "Restored package index from cache {url}",
                            url = style::details(style::url(
                                &updated_package_index.package_index_url
                            ))
                        ),
                        UpdatedSourceCacheState::New => format!(
                            "Downloaded package index {url}",
                            url = style::url(&updated_package_index.package_index_url)
                        ),
                        UpdatedSourceCacheState::Invalidated(reason) => format!(
                            "Redownloaded package index {url} {reason}",
                            url = style::url(&updated_package_index.package_index_url),
                            reason = style::details(reason)
                        ),
                    })
                },
            )
        })
        .done();

    let log = log.bullet("Building package index");
    let timer = log.start_timer("Processing package files");
    let package_index = build_package_index(
        updated_sources
            .into_iter()
            .flat_map(|updated_source| updated_source.package_indexes)
            .collect(),
    )
    .await?;
    let log = timer.done();

    info!(
        { PACKAGE_INDEX_SIZE } = package_index.packages_indexed,
        "package index"
    );
    let log = log
        .sub_bullet(format!(
            "Indexed {} packages",
            package_index.packages_indexed
        ))
        .done();

    Ok((package_index, log))
}

#[instrument(skip_all)]
async fn update_sources(
    context: &Arc<BuildContext<DebianPackagesBuildpack>>,
    client: &ClientWithMiddleware,
    sources: &[Source],
) -> BuildpackResult<Vec<UpdatedSource>> {
    if sources.is_empty() {
        Err(CreatePackageIndexError::NoSources)?;
    }

    let mut update_source_handles = JoinSet::new();

    for source in sources {
        for suite in &source.suites {
            update_source_handles.spawn(
                update_source(
                    context.clone(),
                    client.clone(),
                    source.uri.clone(),
                    suite.to_string(),
                    source.components.clone(),
                    source.arch.clone(),
                    source.signed_by.to_string(),
                )
                .in_current_span(),
            );
        }
    }

    let mut updated_sources = vec![];
    while let Some(update_source_handle) = update_source_handles.join_next().await {
        let updated_source =
            update_source_handle.map_err(CreatePackageIndexError::TaskFailed)??;
        updated_sources.push(updated_source);
    }

    Ok(updated_sources)
}

#[instrument(skip_all)]
async fn update_source(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    repository_uri: RepositoryUri,
    suite: String,
    components: Vec<String>,
    arch: ArchitectureName,
    signed_by: String,
) -> BuildpackResult<UpdatedSource> {
    let updated_release_file = get_release(
        context.clone(),
        client.clone(),
        repository_uri.clone(),
        suite.clone(),
        signed_by,
    )
    .await?;

    let release = async_read_to_string(&updated_release_file.release_file_path)
        .await
        .map_err(|e| {
            CreatePackageIndexError::ReadReleaseFile(
                updated_release_file.release_file_path.clone(),
                e,
            )
        })
        .and_then(|release_data| {
            Release::from(&release_data).map_err(|e| {
                CreatePackageIndexError::ParseReleaseFile(
                    updated_release_file.release_file_path.clone(),
                    e,
                )
            })
        })?;

    let mut get_package_list_handles = JoinSet::new();

    for component in components {
        let package_index = format!("{component}/binary-{arch}/Packages.gz");
        let package_index_release_hash = release
            .sha256sum
            .as_ref()
            .ok_or(CreatePackageIndexError::MissingSha256ReleaseHashes(
                repository_uri.clone(),
            ))?
            .iter()
            .find(|release_hash| release_hash.filename == package_index)
            .ok_or(CreatePackageIndexError::MissingPackageIndexReleaseHash(
                repository_uri.clone(),
                package_index,
            ))?;

        get_package_list_handles.spawn(
            get_package_list(
                context.clone(),
                client.clone(),
                repository_uri.clone(),
                release.acquire_by_hash.unwrap_or_default(),
                suite.clone(),
                component.clone(),
                arch.clone(),
                package_index_release_hash.hash.to_string(),
            )
            .in_current_span(),
        );
    }

    let mut updated_package_indexes = vec![];
    while let Some(get_package_list_handle) = get_package_list_handles.join_next().await {
        updated_package_indexes
            .push(get_package_list_handle.map_err(CreatePackageIndexError::TaskFailed)??);
    }

    Ok(UpdatedSource {
        release_file: updated_release_file,
        package_indexes: updated_package_indexes,
    })
}

#[instrument(skip_all)]
async fn get_release(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    uri: RepositoryUri,
    suite: String,
    signed_by: String,
) -> BuildpackResult<UpdatedReleaseFile> {
    info!({ RELEASE_URI } = %uri, { RELEASE_SUITE } = %suite, "release info");

    let release_file_url = format!("{}/dists/{suite}/InRelease", uri.as_str());

    let response = client
        .get(&release_file_url)
        .send()
        .await
        .and_then(|res| res.error_for_status().map_err(Reqwest))
        .map_err(CreatePackageIndexError::GetReleaseRequest)?;

    // it would be nice to use the url as the layer name but urls don't make for good file names
    // so instead we'll convert the url to a sha256 hex value
    let layer_name = LayerName::from_str(&format!("{:x}", Sha256::digest(&release_file_url)))
        .map_err(|e| CreatePackageIndexError::InvalidLayerName(release_file_url.clone(), e))?;

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

    let cache_state = match release_file_layer.state {
        LayerState::Restored { .. } => UpdatedSourceCacheState::Cached,
        LayerState::Empty { cause } => {
            release_file_layer.write_metadata(new_metadata)?;

            let raw_release_url_path = release_file_layer.path().join(".url");
            async_write(&raw_release_url_path, &release_file_url)
                .await
                .map_err(|e| CreatePackageIndexError::WriteReleaseLayer(raw_release_url_path, e))?;

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

            match cause {
                EmptyLayerCause::NewlyCreated => UpdatedSourceCacheState::New,
                EmptyLayerCause::InvalidMetadataAction { .. } => {
                    UpdatedSourceCacheState::Invalidated("Invalid metadata".to_string())
                }
                EmptyLayerCause::RestoredLayerAction { .. } => {
                    UpdatedSourceCacheState::Invalidated("Stored ETag did not match".to_string())
                }
            }
        }
    };

    Ok(UpdatedReleaseFile {
        release_file_url,
        release_file_path,
        cache_state,
    })
}

#[instrument(skip_all)]
async fn get_package_list(
    context: Arc<BuildContext<DebianPackagesBuildpack>>,
    client: ClientWithMiddleware,
    repository_uri: RepositoryUri,
    acquire_by_hash: bool,
    suite: String,
    component: String,
    arch: ArchitectureName,
    hash: String,
) -> BuildpackResult<UpdatedPackageIndex> {
    info!(
        { PACKAGE_LIST_URI } = %repository_uri,
        { PACKAGE_LIST_SUITE } = %suite,
        { PACKAGE_LIST_COMPONENT } = %component,
        { PACKAGE_LIST_ARCH } = %arch,
        { PACKAGE_LIST_ACQUIRE_BY_HASH } = %acquire_by_hash,
        "package list info"
    );

    let package_index_url = if acquire_by_hash {
        format!(
            "{}/dists/{suite}/{component}/binary-{arch}/by-hash/SHA256/{}",
            repository_uri.as_str(),
            hash
        )
    } else {
        format!(
            "{}/dists/{suite}/{component}/binary-{arch}/Packages.gz",
            repository_uri.as_str()
        )
    };

    // it would be nice to use the url as the layer name but urls don't make for good file names
    // so instead we'll convert the url to a sha256 hex value
    let layer_name = LayerName::from_str(&format!("{:x}", Sha256::digest(&package_index_url)))
        .map_err(|e| CreatePackageIndexError::InvalidLayerName(package_index_url.clone(), e))?;

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

    let cache_state = match package_index_layer.state {
        LayerState::Restored { .. } => UpdatedSourceCacheState::Cached,
        LayerState::Empty { cause } => {
            package_index_layer.write_metadata(new_metadata)?;

            let package_index_url_path = package_index_layer.path().join(".url");
            async_write(&package_index_url_path, &package_index_url)
                .await
                .map_err(|e| {
                    CreatePackageIndexError::WritePackagesLayer(package_index_url_path, e)
                })?;

            let response = client
                .get(&package_index_url)
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

            // Enable support for multistream gz files. In this mode, the reader expects the input to
            // be a sequence of individually gzipped data streams, each with its own header and trailer,
            // ending at EOF. This is standard behavior for gzip readers.
            reader.multiple_members(true);

            let mut writer = AsyncFile::create(&package_index_path).await.map_err(|e| {
                CreatePackageIndexError::WritePackagesLayer(package_index_path.clone(), e)
            })?;

            async_copy(&mut reader, &mut writer).await.map_err(|e| {
                CreatePackageIndexError::WritePackageIndexFromResponse(
                    package_index_path.clone(),
                    e,
                )
            })?;

            writer.flush().await.map_err(|e| {
                CreatePackageIndexError::WritePackageIndexFromResponse(
                    package_index_path.clone(),
                    e,
                )
            })?;

            let calculated_hash = format!("{:x}", hasher.finalize());

            if hash != calculated_hash {
                Err(CreatePackageIndexError::ChecksumFailed {
                    url: package_index_url.clone(),
                    expected: hash,
                    actual: calculated_hash,
                })?;
            }

            match cause {
                EmptyLayerCause::NewlyCreated => UpdatedSourceCacheState::New,
                EmptyLayerCause::InvalidMetadataAction { .. } => {
                    UpdatedSourceCacheState::Invalidated("Invalid metadata".to_string())
                }
                EmptyLayerCause::RestoredLayerAction { .. } => {
                    UpdatedSourceCacheState::Invalidated(
                        "Stored checksum did not match".to_string(),
                    )
                }
            }
        }
    };

    Ok(UpdatedPackageIndex {
        repository_uri,
        package_index_path,
        package_index_url,
        cache_state,
    })
}

#[instrument(skip_all)]
async fn build_package_index(
    updated_sources: Vec<UpdatedPackageIndex>,
) -> BuildpackResult<PackageIndex> {
    let mut get_packages_handles = JoinSet::new();
    for update_source in updated_sources {
        get_packages_handles.spawn(read_packages(update_source).in_current_span());
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
#[instrument(skip_all)]
async fn read_packages(
    updated_source: UpdatedPackageIndex,
) -> BuildpackResult<Vec<RepositoryPackage>> {
    let contents = async_read_to_string(&updated_source.package_index_path)
        .await
        .map_err(|e| {
            CreatePackageIndexError::ReadPackagesFile(updated_source.package_index_path.clone(), e)
        })?
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
                RepositoryPackage::parse_parallel(
                    updated_source.repository_uri.clone(),
                    package_data,
                )
                .map_or_else(Either::Left, Either::Right)
            });
        let _ = send.send((packages, errors));
    });
    let (packages, errors) = recv.await.map_err(CreatePackageIndexError::CpuTaskFailed)?;
    if errors.is_empty() {
        info!(
            { PACKAGE_LIST_SIZE } = packages.len(),
            "parsed package list"
        );
        Ok(packages)
    } else {
        Err(
            CreatePackageIndexError::ParsePackages(updated_source.package_index_path, errors)
                .into(),
        )
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

#[derive(Debug)]
struct UpdatedSource {
    release_file: UpdatedReleaseFile,
    package_indexes: Vec<UpdatedPackageIndex>,
}

#[derive(Debug)]
enum UpdatedSourceCacheState {
    Cached,
    New,
    Invalidated(String),
}

#[derive(Debug)]
struct UpdatedReleaseFile {
    release_file_url: String,
    release_file_path: PathBuf,
    cache_state: UpdatedSourceCacheState,
}

#[derive(Debug)]
struct UpdatedPackageIndex {
    repository_uri: RepositoryUri,
    package_index_path: PathBuf,
    package_index_url: String,
    cache_state: UpdatedSourceCacheState,
}

impl Display for UpdatedSourceCacheState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdatedSourceCacheState::Cached => write!(f, "cached"),
            UpdatedSourceCacheState::New => write!(f, "new"),
            UpdatedSourceCacheState::Invalidated(reason) => {
                write!(f, "updated {}", style::details(reason))
            }
        }
    }
}
