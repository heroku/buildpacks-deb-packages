use crate::aptfile::Aptfile;
use crate::commands::dpkg::DpkgCommand;
use crate::errors::AptBuildpackError;
use crate::layers::installed_packages::{InstalledPackagesLayer, InstalledPackagesState};
use crate::non_root_apt::NonRootApt;
use commons::output::build_log::{BuildLog, Logger};
use commons::output::fmt;
use commons::output::section_log::{log_step, log_step_stream};
use fun_run::CommandWithName;
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::layer_name;
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};
use std::fs;
use std::io::stdout;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;
use walkdir::WalkDir;

#[cfg(test)]
use libcnb_test as _;

mod aptfile;
mod commands;
mod errors;
mod layers;
mod non_root_apt;

buildpack_main!(AptBuildpack);

const BUILDPACK_NAME: &str = "Heroku Apt Buildpack";

const APTFILE_PATH: &str = "Aptfile";

struct AptBuildpack;

impl Buildpack for AptBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = AptBuildpackError;

    fn detect(&self, context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        let exists = context
            .app_dir
            .join(APTFILE_PATH)
            .try_exists()
            .map_err(AptBuildpackError::DetectAptfile)?;

        if exists {
            DetectResultBuilder::pass().build()
        } else {
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        let mut logger = BuildLog::new(stdout()).buildpack_name(BUILDPACK_NAME);

        let force_yes_requirement =
            semver::VersionReq::parse("<=1.0").expect("this should be a valid semver range");

        let aptfile: Aptfile = fs::read_to_string(context.app_dir.join(APTFILE_PATH))
            .map_err(AptBuildpackError::ReadAptfile)?
            .parse()
            .map_err(|_| AptBuildpackError::ParseAptfile)?;

        let mut section = logger.section("Apt packages cache");
        let cache_restored = AtomicBool::new(false);
        let installed_packages_cache_state = context
            .handle_layer(
                layer_name!("installed_packages"),
                InstalledPackagesLayer {
                    aptfile: &aptfile,
                    cache_restored: &cache_restored,
                    _section_logger: section.as_ref(),
                },
            )
            .map(|layer| {
                if cache_restored.into_inner() {
                    InstalledPackagesState::Restored
                } else {
                    InstalledPackagesState::New(layer.path)
                }
            })?;
        logger = section.end_section();

        section = logger.section("Installing packages from Aptfile");
        if let InstalledPackagesState::New(install_path) = installed_packages_cache_state {
            let non_root_apt = NonRootApt::new()
                .map(|non_root_apt_installation| {
                    log_step(format!(
                        "Using apt version {}",
                        fmt::value(non_root_apt_installation.apt_version.to_string())
                    ));
                    non_root_apt_installation
                })
                .map_err(AptBuildpackError::CreateNonRootApt)?;

            let apt_get_update = non_root_apt.apt_get().update();
            let mut command: Command = apt_get_update.into();
            log_step_stream(
                format!("Running {}", fmt::command(command.name())),
                |stream| command.stream_output(stream.io(), stream.io()),
            )
            .map_err(AptBuildpackError::AptGetUpdate)?;

            for package in &aptfile.packages {
                let mut apt_get = non_root_apt.apt_get();
                apt_get.assume_yes = true;
                apt_get.download_only = true;
                apt_get.reinstall = true;

                if force_yes_requirement.matches(&non_root_apt.apt_version) {
                    apt_get.force_yes = true;
                } else {
                    apt_get.allow_downgrades = true;
                    apt_get.allow_remove_essential = true;
                    apt_get.allow_change_held_packages = true;
                };

                let mut apt_get_install = apt_get.install();
                apt_get_install.packages.insert(package.clone());

                let mut command: Command = apt_get_install.into();
                log_step_stream(
                    format!("Running {}", fmt::command(command.name())),
                    |stream| command.stream_output(stream.io(), stream.io()),
                )
                .map_err(|e| AptBuildpackError::DownloadPackage(package.clone(), e))?;
            }

            let debian_archives = non_root_apt
                .list_downloaded_debian_packages()
                .map_err(AptBuildpackError::ListDownloadedPackages)?;

            for archive in debian_archives {
                let mut dpkg = DpkgCommand::new();
                dpkg.extract(archive.clone(), install_path.clone());
                let mut command: Command = dpkg.into();
                log_step_stream(
                    format!("Running {}", fmt::command(command.name())),
                    |stream| command.stream_output(stream.io(), stream.io()),
                )
                .map_err(|e| AptBuildpackError::InstallPackage(archive.clone(), e))?;
            }

            log_step("Rewrite package-config files");
            for entry in WalkDir::new(&install_path).into_iter().flatten() {
                let path = entry.path();
                let is_package_config_file = entry.file_type().is_file()
                    && package_config_file_name_regex().is_match(&entry.path().to_string_lossy());
                if is_package_config_file {
                    let contents = fs::read_to_string(path).map_err(|e| {
                        AptBuildpackError::ReadPackageConfigFile(path.to_path_buf(), e)
                    })?;
                    let new_contents = contents
                        .split('\n')
                        .map(|line| {
                            let prefix_match = package_config_prefix_line_regex()
                                .captures(line)
                                .and_then(|captures| captures.get(1));
                            if let Some(prefix_value) = prefix_match {
                                format!(
                                    "prefix={}",
                                    install_path.join(prefix_value.as_str()).to_string_lossy()
                                )
                            } else {
                                line.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    fs::write(path, new_contents).map_err(|e| {
                        AptBuildpackError::WritePackageConfigFile(path.to_path_buf(), e)
                    })?;
                }
            }
        } else {
            log_step("Skipping, packages already in cache");
        }
        logger = section.end_section();

        logger.finish_logging();
        BuildResultBuilder::new().build()
    }
}

fn package_config_file_name_regex() -> &'static regex_lite::Regex {
    static LAZY: OnceLock<regex_lite::Regex> = OnceLock::new();
    LAZY.get_or_init(|| {
        regex_lite::Regex::new(r"^.*/pkgconfig/.*\.pc$").expect("should be a valid regex pattern")
    })
}

fn package_config_prefix_line_regex() -> &'static regex_lite::Regex {
    static LAZY: OnceLock<regex_lite::Regex> = OnceLock::new();
    LAZY.get_or_init(|| {
        regex_lite::Regex::new(r"^prefix=(.*)$").expect("should be a valid regex pattern")
    })
}
