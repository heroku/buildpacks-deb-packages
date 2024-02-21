use std::fs;
use std::io::stdout;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use commons::output::build_log::{BuildLog, Logger};
use commons::output::fmt;
use commons::output::section_log::{log_step_stream, log_step_timed};
use fun_run::CommandWithName;
use indoc::formatdoc;
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::layer_name;
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};
#[cfg(test)]
use libcnb_test as _;
#[cfg(test)]
use regex_lite as _;
use tempfile::{tempdir, TempDir};

use crate::aptfile::Aptfile;
use crate::commands::apt::{AptGetCommand, AptVersion};
use crate::debian::DebianArchitectureName;
use crate::dependency_set::DependencySet;
use crate::dependency_versions::DependencyVersions;
use crate::errors::AptBuildpackError;
use crate::layers::environment::EnvironmentLayer;
use crate::layers::installed_packages::InstalledPackagesLayer;

mod aptfile;
mod commands;
mod debian;
mod dependency_set;
mod dependency_versions;
mod errors;
mod layers;

buildpack_main!(AptBuildpack);

const BUILDPACK_NAME: &str = "Heroku Apt Buildpack";

const APTFILE_PATH: &str = "Aptfile";

struct AptBuildpack;

impl Buildpack for AptBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = AptBuildpackError;

    fn detect(&self, context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        let aptfile_exists = context
            .app_dir
            .join(APTFILE_PATH)
            .try_exists()
            .map_err(AptBuildpackError::DetectAptfile)?;

        if aptfile_exists {
            DetectResultBuilder::pass().build()
        } else {
            BuildLog::new(stdout())
                .without_buildpack_name()
                .announce()
                .warning("No Aptfile found.");
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        let logger = BuildLog::new(stdout()).buildpack_name(BUILDPACK_NAME);

        let aptfile: Aptfile = fs::read_to_string(context.app_dir.join(APTFILE_PATH))
            .map_err(AptBuildpackError::ReadAptfile)?
            .parse()
            .map_err(AptBuildpackError::ParseAptfile)?;

        let debian_architecture_name = DebianArchitectureName::from_str(&context.target.arch)
            .map_err(AptBuildpackError::ParseDebianArchitectureName)?;

        let apt_dir = create_apt_dir()?;

        let apt_config = create_apt_config(apt_dir.path())?;

        let apt_version = get_apt_version()?;

        let section = logger.section("Apt packages");

        update_apt_sources(&apt_config)?;

        let dependency_versions = log_step_timed("Collecting dependency information", || {
            DependencySet::from_apt_cache_depends(aptfile.packages.clone(), apt_config.clone())
                .map_err(AptBuildpackError::DependencySet)
                .and_then(|dependency_set| {
                    DependencyVersions::from_apt_cache_policy(
                        dependency_set.deref().clone(),
                        apt_config.clone(),
                    )
                    .map_err(AptBuildpackError::DependencyVersions)
                })
        })?;

        let installed_packages_layer_data = context.handle_layer(
            layer_name!("installed_packages"),
            InstalledPackagesLayer {
                aptfile: &aptfile,
                apt_dir: &apt_dir,
                apt_config: &apt_config,
                apt_version: &apt_version,
                dependency_versions: &dependency_versions,
                _section_logger: section.as_ref(),
            },
        )?;

        context.handle_layer(
            layer_name!("environment"),
            EnvironmentLayer {
                debian_architecture_name: &debian_architecture_name,
                installed_packages_dir: &installed_packages_layer_data.path,
                _section_logger: section.as_ref(),
            },
        )?;

        section.end_section().finish_logging();

        BuildResultBuilder::new().build()
    }
}

fn create_apt_dir() -> Result<TempDir, AptBuildpackError> {
    let apt_dir = tempdir().map_err(AptBuildpackError::CreateAptDir)?;
    // apt-get complains if these folders aren't present
    fs::create_dir_all(apt_dir.path().join("state/lists/partial"))
        .map_err(AptBuildpackError::CreateAptDir)?;
    fs::create_dir_all(apt_dir.path().join("cache/archives/partial"))
        .map_err(AptBuildpackError::CreateAptDir)?;
    Ok(apt_dir)
}

fn create_apt_config(apt_dir: &Path) -> Result<PathBuf, AptBuildpackError> {
    // https://manpages.ubuntu.com/manpages/jammy/man5/apt.conf.5.html
    // set a custom apt.conf so that our calls to apt-get use our custom installation
    let apt_config = apt_dir.join("apt.conf");
    fs::write(
        apt_dir.join("apt.conf"),
        formatdoc! { r#"
            #clear APT::Update::Post-Invoke;
            Debug::NoLocking "true";
            Dir::Cache "{apt_dir}/cache";
            Dir::State "{apt_dir}/state";      
        "#, apt_dir = apt_dir.to_string_lossy() },
    )
    .map_err(AptBuildpackError::CreateAptConfig)
    .map(|()| apt_config)
}

fn get_apt_version() -> Result<AptVersion, AptBuildpackError> {
    let mut apt_get = AptGetCommand::new();
    apt_get.version = true;
    Command::from(apt_get)
        .named_output()
        .map_err(AptBuildpackError::AptGetVersionCommand)
        .and_then(|output| {
            output
                .stdout_lossy()
                .parse::<AptVersion>()
                .map_err(AptBuildpackError::ParseAptGetVersion)
        })
}

fn update_apt_sources(config_file: &Path) -> Result<(), AptBuildpackError> {
    let mut apt_get = AptGetCommand::new();
    apt_get.config_file = Some(config_file.to_path_buf());
    let apt_get_update = apt_get.update();
    let mut command = Command::from(apt_get_update);
    log_step_stream(
        format!("Updating sources with {}", fmt::command(command.name())),
        |stream| command.stream_output(stream.io(), stream.io()),
    )
    .map_err(AptBuildpackError::AptGetUpdate)
    .map(|_| ())
}
