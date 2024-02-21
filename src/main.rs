use crate::aptfile::Aptfile;
use crate::debian::DebianArchitectureName;
use crate::errors::AptBuildpackError;
use crate::layers::environment::EnvironmentLayer;
use crate::layers::installed_packages::InstalledPackagesLayer;
use commons::output::build_log::{BuildLog, Logger};
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::layer_name;
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};
#[cfg(test)]
use libcnb_test as _;
#[cfg(test)]
use regex_lite as _;
use std::fs;
use std::io::stdout;
use std::str::FromStr;

mod aptfile;
mod commands;
mod debian;
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

        let section = logger.section("Apt packages");

        let installed_packages_layer_data = context.handle_layer(
            layer_name!("installed_packages"),
            InstalledPackagesLayer {
                aptfile: &aptfile,
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
