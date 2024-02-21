use crate::aptfile::ParseAptfileError;
use crate::commands::apt_get::ParseAptVersionError;
use crate::debian::ParseDebianArchitectureNameError;
use std::path::PathBuf;

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum AptBuildpackError {
    DetectAptfile(std::io::Error),
    ReadAptfile(std::io::Error),
    ParseAptfile(ParseAptfileError),
    ParseDebianArchitectureName(ParseDebianArchitectureNameError),
    CreateAptDir(std::io::Error),
    CreateAptConfig(std::io::Error),
    AptGetVersionCommand(fun_run::CmdError),
    ParseAptGetVersion(ParseAptVersionError),
    AptGetUpdate(fun_run::CmdError),
    DownloadPackages(fun_run::CmdError),
    ListDownloadedPackages(std::io::Error),
    InstallPackage(PathBuf, fun_run::CmdError),
}

impl From<AptBuildpackError> for libcnb::Error<AptBuildpackError> {
    fn from(value: AptBuildpackError) -> Self {
        Self::BuildpackError(value)
    }
}
