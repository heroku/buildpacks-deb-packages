use crate::debian::{Distro, MultiarchName, RepositoryPackage};
use crate::o11y::*;
use crate::{
    is_buildpack_debug_logging_enabled, BuildpackResult, DebianPackagesBuildpack,
    DebianPackagesBuildpackError,
};
use ar::Archive as ArArchive;
use async_compression::tokio::bufread::{GzipDecoder, XzDecoder, ZstdDecoder};
use bullet_stream::state::{Bullet, SubBullet};
use bullet_stream::{style, Print};
use futures::io::AllowStdIo;
use futures::TryStreamExt;
use indexmap::IndexSet;
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer::{
    CachedLayerDefinition, EmptyLayerCause, InvalidMetadataAction, LayerState, RestoredLayerAction,
};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_middleware::Error::Reqwest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env::temp_dir;
use std::ffi::OsString;
use std::fs::File;
use std::io::{ErrorKind, Stdout, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{read_to_string as async_read_to_string, write as async_write, File as AsyncFile};
use tokio::io::{copy as async_copy, BufReader as AsyncBufReader, BufWriter as AsyncBufWriter};
use tokio::task::{JoinError, JoinSet};
use tokio_tar::Archive as TarArchive;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::io::InspectReader;
use tracing::{info, instrument, Instrument};
use walkdir::{DirEntry, WalkDir};

#[instrument(skip_all)]
pub(crate) async fn install_packages(
    context: &Arc<BuildContext<DebianPackagesBuildpack>>,
    client: &ClientWithMiddleware,
    distro: &Distro,
    packages_to_install: Vec<RepositoryPackage>,
    mut log: Print<Bullet<Stdout>>,
) -> BuildpackResult<Print<Bullet<Stdout>>> {
    log = log.h2("Installing packages");

    let new_metadata = InstallationMetadata {
        package_checksums: packages_to_install
            .iter()
            .map(|package| (package.name.to_string(), package.sha256sum.to_string()))
            .collect(),
        distro: distro.clone(),
    };

    let install_layer = context.cached_layer(
        layer_name!("packages"),
        CachedLayerDefinition {
            build: true,
            launch: true,
            invalid_metadata_action: &|_| InvalidMetadataAction::DeleteLayer,
            restored_layer_action: &|old_metadata: &InstallationMetadata, _| {
                if old_metadata == &new_metadata {
                    RestoredLayerAction::KeepLayer
                } else {
                    RestoredLayerAction::DeleteLayer
                }
            },
        },
    )?;

    match install_layer.state {
        LayerState::Restored { .. } => {
            info!(deb_packages.install_layer_cached = true);
            log = packages_to_install
                .iter()
                .fold(
                    log.bullet("Restoring packages from cache"),
                    |log, package_to_install| {
                        log.sub_bullet(style::value(format!(
                            "{name}@{version}",
                            name = package_to_install.name,
                            version = package_to_install.version
                        )))
                    },
                )
                .done();
        }
        LayerState::Empty { cause } => {
            info!(deb_packages.install_layer_cached = false);
            let install_log = packages_to_install.iter().fold(
                log.bullet(match cause {
                    EmptyLayerCause::NewlyCreated => "Requesting packages",
                    EmptyLayerCause::InvalidMetadataAction { .. } => {
                        "Requesting packages (invalid metadata)"
                    }
                    EmptyLayerCause::RestoredLayerAction { .. } => {
                        "Requesting packages (packages changed)"
                    }
                }),
                |log, package_to_install| {
                    log.sub_bullet(format!(
                        "{name_with_version} from {url}",
                        name_with_version = style::value(format!(
                            "{name}@{version}",
                            name = package_to_install.name,
                            version = package_to_install.version
                        )),
                        url = style::url(build_download_url(package_to_install))
                    ))
                },
            );

            let timer = install_log.start_timer("Downloading");
            install_layer.write_metadata(new_metadata)?;

            let mut download_and_extract_handles = JoinSet::new();

            for repository_package in packages_to_install {
                download_and_extract_handles.spawn(
                    download_and_extract(
                        client.clone(),
                        repository_package.clone(),
                        install_layer.path(),
                    )
                    .in_current_span(),
                );
            }

            while let Some(download_and_extract_handle) =
                download_and_extract_handles.join_next().await
            {
                download_and_extract_handle.map_err(InstallPackagesError::TaskFailed)??;
            }

            log = timer.done().done();
        }
    }

    let layer_env = configure_layer_environment(
        &install_layer.path(),
        &MultiarchName::from(&distro.architecture),
    );

    install_layer.write_env(layer_env)?;

    rewrite_package_configs(&install_layer.path()).await?;

    let mut install_log = log.bullet("Installation complete");
    if is_buildpack_debug_logging_enabled() {
        install_log = print_layer_contents(&install_layer.path(), install_log);
    }
    log = install_log.done();

    Ok(log)
}

fn print_layer_contents(
    install_path: &Path,
    log: Print<SubBullet<Stdout>>,
) -> Print<SubBullet<Stdout>> {
    let mut directory_log = log.start_stream("Layer file listing");
    WalkDir::new(install_path)
        .into_iter()
        .flatten()
        .filter(|entry| {
            // filter out the env layer that's created for CNB environment files
            if let Some(parent) = entry.path().parent() {
                if parent == install_path.join("env") {
                    return false;
                }
            }
            entry.file_type().is_file()
        })
        .map(|entry| entry.path().to_path_buf())
        .for_each(|path| {
            let _ = writeln!(&mut directory_log, "{}", path.to_string_lossy());
        });
    let _ = writeln!(&mut directory_log);
    directory_log.done()
}

#[instrument(skip_all)]
async fn download_and_extract(
    client: ClientWithMiddleware,
    repository_package: RepositoryPackage,
    install_dir: PathBuf,
) -> BuildpackResult<()> {
    let download_path = download(client, &repository_package).await?;
    extract(download_path, install_dir).await
}

#[instrument(skip_all)]
async fn download(
    client: ClientWithMiddleware,
    repository_package: &RepositoryPackage,
) -> BuildpackResult<PathBuf> {
    info!(
        { DOWNLOAD_PACKAGE_NAME } = %repository_package.name,
        { DOWNLOAD_PACKAGE_VERSION } = %repository_package.version,
        "downloading package"
    );
    let download_url = build_download_url(repository_package);

    let download_file_name = PathBuf::from(repository_package.filename.as_str())
        .file_name()
        .map(ToOwned::to_owned)
        .ok_or(InstallPackagesError::InvalidFilename(
            repository_package.name.clone(),
            repository_package.filename.clone(),
        ))?;

    let download_path = temp_dir().join::<&Path>(download_file_name.as_ref());

    let response = client
        .get(&download_url)
        .send()
        .await
        .and_then(|res| res.error_for_status().map_err(Reqwest))
        .map_err(|e| InstallPackagesError::RequestPackage(repository_package.clone(), e))?;

    let mut hasher = Sha256::new();

    let mut writer = AsyncFile::create(&download_path)
        .await
        .map_err(|e| {
            InstallPackagesError::WritePackage(
                repository_package.clone(),
                download_url.clone(),
                download_path.clone(),
                e,
            )
        })
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

    async_copy(&mut reader, &mut writer).await.map_err(|e| {
        InstallPackagesError::WritePackage(
            repository_package.clone(),
            download_url.clone(),
            download_path.clone(),
            e,
        )
    })?;

    let calculated_hash = format!("{:x}", hasher.finalize());
    let hash = repository_package.sha256sum.to_string();

    if hash != calculated_hash {
        Err(InstallPackagesError::ChecksumFailed {
            url: download_url,
            expected: hash,
            actual: calculated_hash,
        })?;
    }

    Ok(download_path)
}

#[instrument(skip_all)]
async fn extract(download_path: PathBuf, output_dir: PathBuf) -> BuildpackResult<()> {
    // a .deb file is an ar archive
    // https://manpages.ubuntu.com/manpages/jammy/en/man5/deb.5.html
    let mut debian_archive = File::open(&download_path)
        .map_err(|e| InstallPackagesError::OpenPackageArchive(download_path.clone(), e))
        .map(ArArchive::new)?;

    while let Some(entry) = debian_archive.next_entry() {
        let entry = entry
            .map_err(|e| InstallPackagesError::OpenPackageArchiveEntry(download_path.clone(), e))?;
        let entry_path = PathBuf::from(OsString::from_vec(entry.header().identifier().to_vec()));
        let entry_reader =
            AsyncBufReader::new(FuturesAsyncReadCompatExt::compat(AllowStdIo::new(entry)));

        // https://manpages.ubuntu.com/manpages/noble/en/man5/deb.5.html
        match (
            entry_path.file_stem().and_then(|v| v.to_str()),
            entry_path.extension().and_then(|v| v.to_str()),
        ) {
            (Some("data.tar"), Some("gz")) => {
                info!({ EXTRACT_PACKAGE_DECODER } = "gzip", "extract package");
                let mut tar_archive = TarArchive::new(GzipDecoder::new(entry_reader));
                tar_archive
                    .unpack(&output_dir)
                    .await
                    .map_err(|e| InstallPackagesError::UnpackTarball(download_path.clone(), e))?;
            }
            (Some("data.tar"), Some("zstd" | "zst")) => {
                info!({ EXTRACT_PACKAGE_DECODER } = "zstd", "extract package");
                let mut tar_archive = TarArchive::new(ZstdDecoder::new(entry_reader));
                tar_archive
                    .unpack(&output_dir)
                    .await
                    .map_err(|e| InstallPackagesError::UnpackTarball(download_path.clone(), e))?;
            }
            (Some("data.tar"), Some("xz")) => {
                info!({ EXTRACT_PACKAGE_DECODER } = "xz", "extract package");
                let mut tar_archive = TarArchive::new(XzDecoder::new(entry_reader));
                tar_archive
                    .unpack(&output_dir)
                    .await
                    .map_err(|e| InstallPackagesError::UnpackTarball(download_path.clone(), e))?;
            }
            (Some("data.tar"), Some(compression)) => {
                info!({ EXTRACT_PACKAGE_DECODER } = compression, "extract package");
                Err(InstallPackagesError::UnsupportedCompression(
                    download_path.clone(),
                    compression.to_string(),
                ))?;
            }
            _ => {
                // ignore other potential file entries (e.g.; debian-binary, control.tar)
            }
        }
    }

    Ok(())
}

#[instrument(skip_all)]
fn configure_layer_environment(install_path: &Path, multiarch_name: &MultiarchName) -> LayerEnv {
    let mut layer_env = LayerEnv::new();

    let bin_paths = [
        install_path.join("bin"),
        install_path.join("usr/bin"),
        install_path.join("usr/sbin"),
    ];
    prepend_to_env_var(&mut layer_env, "PATH", &bin_paths);

    // support multi-arch and legacy filesystem layouts for debian packages
    // https://wiki.ubuntu.com/MultiarchSpec
    let library_paths = [
        install_path.join(format!("usr/lib/{multiarch_name}")),
        install_path.join("usr/lib"),
        install_path.join(format!("lib/{multiarch_name}")),
        install_path.join("lib"),
    ]
    .iter()
    .fold(IndexSet::new(), |mut acc, lib_dir| {
        for dir in find_all_dirs_containing(lib_dir, shared_library_file) {
            acc.insert(dir);
        }
        acc.insert(lib_dir.clone());
        acc
    });
    prepend_to_env_var(&mut layer_env, "LD_LIBRARY_PATH", &library_paths);
    prepend_to_env_var(&mut layer_env, "LIBRARY_PATH", &library_paths);

    let include_paths = [
        install_path.join(format!("usr/include/{multiarch_name}")),
        install_path.join("usr/include"),
    ]
    .iter()
    .fold(IndexSet::new(), |mut acc, include_dir| {
        for dir in find_all_dirs_containing(include_dir, header_file) {
            acc.insert(dir);
        }
        acc.insert(include_dir.clone());
        acc
    });
    prepend_to_env_var(&mut layer_env, "INCLUDE_PATH", &include_paths);
    prepend_to_env_var(&mut layer_env, "CPATH", &include_paths);
    prepend_to_env_var(&mut layer_env, "CPPPATH", &include_paths);

    let pkg_config_paths = [
        install_path.join(format!("usr/lib/{multiarch_name}/pkgconfig")),
        install_path.join("usr/lib/pkgconfig"),
    ];
    prepend_to_env_var(&mut layer_env, "PKG_CONFIG_PATH", &pkg_config_paths);

    info!(
        { ENV_PATH } = as_json_value(&bin_paths),
        { LIBRARY_PATH } = as_json_value(&library_paths.iter().collect::<Vec<_>>()),
        { INCLUDE_PATH } = as_json_value(&include_paths.iter().collect::<Vec<_>>()),
        { PKG_CONFIG_PATH } = as_json_value(&pkg_config_paths.iter().collect::<Vec<_>>()),
        "layer environment"
    );

    layer_env
}

fn find_all_dirs_containing(
    starting_dir: &Path,
    condition: impl Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let mut matches = vec![];
    if let Ok(true) = starting_dir.try_exists() {
        for entry in WalkDir::new(starting_dir).into_iter().flatten() {
            if let Some(parent_dir) = entry.path().parent() {
                if condition(entry.path()) {
                    matches.push(parent_dir.to_path_buf());
                }
            }
        }
    }
    // order the paths by longest to shortest
    matches.sort_by_key(|v| std::cmp::Reverse(v.as_os_str().len()));
    matches
}

fn shared_library_file(path: &Path) -> bool {
    let mut current_path = path.to_path_buf();
    let mut current_ext = current_path.extension();
    let mut current_name = current_path.file_stem();
    while let (Some(name), Some(ext)) = (current_name, current_ext) {
        if ext == "so" {
            return true;
        }
        current_path = PathBuf::from(name);
        current_ext = current_path.extension();
        current_name = current_path.file_stem();
    }
    false
}

fn header_file(path: &Path) -> bool {
    matches!(path.extension(), Some(ext) if ext == "h")
}

fn prepend_to_env_var<I, T>(layer_env: &mut LayerEnv, name: &str, paths: I)
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let separator = ":";
    layer_env.insert(Scope::All, ModificationBehavior::Delimiter, name, separator);
    layer_env.insert(
        Scope::All,
        ModificationBehavior::Prepend,
        name,
        paths
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
            .join(separator.as_ref()),
    );
}

async fn rewrite_package_configs(install_path: &Path) -> BuildpackResult<()> {
    let package_configs = WalkDir::new(install_path)
        .into_iter()
        .flatten()
        .filter(is_package_config)
        .map(|entry| entry.path().to_path_buf());

    for package_config in package_configs {
        rewrite_package_config(&package_config, install_path).await?;
    }

    Ok(())
}

fn is_package_config(entry: &DirEntry) -> bool {
    matches!((
        entry.path().parent().and_then(|p| p.file_name()),
        entry.path().extension()
    ), (Some(parent), Some(ext)) if parent == "pkgconfig" && ext == "pc")
}

async fn rewrite_package_config(package_config: &Path, install_path: &Path) -> BuildpackResult<()> {
    let contents = async_read_to_string(package_config)
        .await
        .map_err(|e| InstallPackagesError::ReadPackageConfig(package_config.to_path_buf(), e))?;

    let new_contents = contents
        .lines()
        .map(|line| {
            if let Some(prefix_value) = line.strip_prefix("prefix=") {
                format!(
                    "prefix={}",
                    install_path
                        .join(prefix_value.trim_start_matches('/'))
                        .to_string_lossy()
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(async_write(package_config, new_contents)
        .await
        .map_err(|e| InstallPackagesError::WritePackageConfig(package_config.to_path_buf(), e))?)
}

fn build_download_url(repository_package: &RepositoryPackage) -> String {
    format!(
        "{}/{}",
        repository_package.repository_uri.as_str(),
        repository_package.filename.as_str()
    )
}

#[derive(Debug)]
pub(crate) enum InstallPackagesError {
    TaskFailed(JoinError),
    InvalidFilename(String, String),
    RequestPackage(RepositoryPackage, reqwest_middleware::Error),
    WritePackage(RepositoryPackage, String, PathBuf, std::io::Error),
    ChecksumFailed {
        url: String,
        expected: String,
        actual: String,
    },
    OpenPackageArchive(PathBuf, std::io::Error),
    OpenPackageArchiveEntry(PathBuf, std::io::Error),
    UnpackTarball(PathBuf, std::io::Error),
    UnsupportedCompression(PathBuf, String),
    ReadPackageConfig(PathBuf, std::io::Error),
    WritePackageConfig(PathBuf, std::io::Error),
}

impl From<InstallPackagesError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: InstallPackagesError) -> Self {
        Self::BuildpackError(DebianPackagesBuildpackError::InstallPackages(value))
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
struct InstallationMetadata {
    package_checksums: HashMap<String, String>,
    distro: Distro,
}

#[cfg(test)]
mod test {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use libcnb::layer_env::Scope;
    use tempfile::TempDir;

    use crate::debian::MultiarchName;
    use crate::install_packages::configure_layer_environment;

    #[test]
    fn configure_layer_environment_adds_nested_directories_with_shared_libraries_to_library_path() {
        let arch = MultiarchName::X86_64_LINUX_GNU;
        let install_dir = create_installation(bon::vec![
            format!("usr/lib/{arch}/nested-1/shared-library.so.2"),
            "usr/lib/nested-2/shared-library.so",
            format!("lib/{arch}/nested-3/shared-library.so.4"),
            format!("lib/{arch}/nested-3/deeply/shared-library.so.5"),
            "lib/nested-4/shared-library.so.3.0.0",
            "usr/lib/nested-but/does-not-contain-a-shared-library.txt",
            "usr/not-a-lib-dir/shared-library.so.6"
        ]);
        let install_path = install_dir.path();
        let layer_env = configure_layer_environment(install_path, &arch);
        assert_eq!(
            split_into_paths(layer_env.apply_to_empty(Scope::All).get("LD_LIBRARY_PATH")),
            vec![
                install_path.join(format!("usr/lib/{arch}/nested-1")),
                install_path.join(format!("usr/lib/{arch}")),
                install_path.join("usr/lib/nested-2"),
                install_path.join("usr/lib"),
                install_path.join(format!("lib/{arch}/nested-3/deeply")),
                install_path.join(format!("lib/{arch}/nested-3")),
                install_path.join(format!("lib/{arch}")),
                install_path.join("lib/nested-4"),
                install_path.join("lib"),
            ]
        );
    }

    #[test]
    fn configure_layer_environment_adds_nested_directories_with_headers_to_include_path() {
        let arch = MultiarchName::X86_64_LINUX_GNU;
        let install_dir = create_installation(bon::vec![
            format!("usr/include/{arch}/nested-1/header.h"),
            "usr/include/nested-2/header.h",
            "usr/include/nested-2/deeply/header.h",
            "usr/include/nested-but/does-not-contain-a-header.txt",
            "usr/not-an-include-dir/header.h"
        ]);
        let install_path = install_dir.path();
        let layer_env = configure_layer_environment(install_path, &arch);
        assert_eq!(
            split_into_paths(layer_env.apply_to_empty(Scope::All).get("INCLUDE_PATH")),
            vec![
                install_path.join(format!("usr/include/{arch}/nested-1")),
                install_path.join(format!("usr/include/{arch}")),
                install_path.join("usr/include/nested-2/deeply"),
                install_path.join("usr/include/nested-2"),
                install_path.join("usr/include"),
            ]
        );
    }

    fn create_installation(files: Vec<String>) -> TempDir {
        let install_dir = tempfile::tempdir().unwrap();
        for file in files {
            create_file(install_dir.path(), &file);
        }
        install_dir
    }

    fn create_file(install_dir: &Path, file: &str) {
        let file_path = install_dir.join(file);
        let dir = file_path.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::File::create(file_path).unwrap();
    }

    fn split_into_paths(env_var: Option<&OsString>) -> Vec<PathBuf> {
        env_var
            .map(|v| {
                v.to_string_lossy()
                    .split(':')
                    .map(PathBuf::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }
}
