use crate::aptfile::ParseAptfileError;
use crate::debian::ParseDebianArchitectureNameError;

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
#[allow(dead_code)] // TODO: remove this once error messages are added
pub(crate) enum AptBuildpackError {
    DetectAptfile(std::io::Error),
    ReadAptfile(std::io::Error),
    ParseAptfile(ParseAptfileError),
    ParseDebianArchitectureName(ParseDebianArchitectureNameError),
}

impl From<AptBuildpackError> for libcnb::Error<AptBuildpackError> {
    fn from(value: AptBuildpackError) -> Self {
        Self::BuildpackError(value)
    }
}
