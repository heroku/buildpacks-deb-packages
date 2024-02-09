#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum AptBuildpackError {
    DetectAptfile(std::io::Error),
    ReadAptfile(std::io::Error),
    ParseAptfile,
}

impl From<AptBuildpackError> for libcnb::Error<AptBuildpackError> {
    fn from(value: AptBuildpackError) -> Self {
        libcnb::Error::BuildpackError(value)
    }
}
