use crate::aptfile::Aptfile;
use crate::errors::AptBuildpackError;
use commons::output::build_log::{BuildLog, Logger};
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};
use std::fs;
use std::io::stdout;

#[cfg(test)]
use libcnb_test as _;

mod aptfile;
mod errors;

buildpack_main!(AptBuildpack);

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
        let _aptfile: Aptfile = fs::read_to_string(context.app_dir.join(APTFILE_PATH))
            .map_err(AptBuildpackError::ReadAptfile)?
            .parse()
            .map_err(AptBuildpackError::ParseAptfile)?;

        BuildResultBuilder::new().build()
    }
}
