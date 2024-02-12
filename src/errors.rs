use crate::aptfile::DebianPackage;
use crate::non_root_apt::NonRootAptError;
use std::path::PathBuf;

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum AptBuildpackError {
    DetectAptfile(std::io::Error),
    ReadAptfile(std::io::Error),
    ParseAptfile,
    CreateNonRootApt(NonRootAptError),
    AptGetUpdate(fun_run::CmdError),
    DownloadPackage(DebianPackage, fun_run::CmdError),
    ListDownloadedPackages(std::io::Error),
    InstallPackage(PathBuf, fun_run::CmdError),
}

impl From<AptBuildpackError> for libcnb::Error<AptBuildpackError> {
    fn from(value: AptBuildpackError) -> Self {
        libcnb::Error::BuildpackError(value)
    }
}
