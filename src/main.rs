use crate::aptfile::Aptfile;
use crate::errors::AptBuildpackError;
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};
use std::fs;

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
        let _aptfile: Aptfile = fs::read_to_string(context.app_dir.join(APTFILE_PATH))
            .map_err(AptBuildpackError::ReadAptfile)?
            .parse()
            .map_err(|_| AptBuildpackError::ParseAptfile)?;

        BuildResultBuilder::new().build()
    }
}
