use crate::errors::AptBuildpackError;
use libcnb::build::{BuildContext, BuildResult};
use libcnb::detect::{DetectContext, DetectResult};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};

mod errors;

buildpack_main!(AptBuildpack);

struct AptBuildpack;

impl Buildpack for AptBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = AptBuildpackError;

    fn detect(&self, _context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        todo!()
    }

    fn build(&self, _context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        todo!()
    }
}
