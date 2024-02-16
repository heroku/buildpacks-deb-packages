use crate::aptfile::ParseAptfileError;

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum AptBuildpackError {
    DetectAptfile(std::io::Error),
    ReadAptfile(std::io::Error),
    ParseAptfile(ParseAptfileError),
}

impl From<AptBuildpackError> for libcnb::Error<AptBuildpackError> {
    fn from(value: AptBuildpackError) -> Self {
        Self::BuildpackError(value)
    }
}
