use crate::aptfile::Aptfile;
use crate::errors::AptBuildpackError;
use crate::layers::installed_packages::{InstalledPackagesLayer, InstalledPackagesState};
use commons::output::build_log::{BuildLog, Logger};
use commons::output::section_log::log_step;
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::layer_name;
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};
use std::fs;
use std::io::stdout;
use std::sync::atomic::AtomicBool;

mod aptfile;
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
        if let InstalledPackagesState::New(_install_path) = installed_packages_cache_state {
            log_step("")
        } else {
            log_step("Skipping, packages already in cache");
        }
        logger = section.end_section();

        logger.finish_logging();
        BuildResultBuilder::new().build()
    }
}
