use crate::aptfile::ParseAptfileError;
use crate::commands::apt_get::ParseAptVersionError;
use crate::debian::ParseDebianArchitectureNameError;
use crate::BUILDPACK_NAME;
use commons::output::build_log::{BuildLog, Logger, StartedLogger};
use commons::output::fmt;
use commons::output::fmt::DEBUG_INFO;
use indoc::formatdoc;
use std::fmt::Display;
use std::io::stdout;
use std::path::{Path, PathBuf};

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
    ReadPackageConfigFile(PathBuf, std::io::Error),
    WritePackageConfigFile(PathBuf, std::io::Error),
}

impl From<AptBuildpackError> for libcnb::Error<AptBuildpackError> {
    fn from(value: AptBuildpackError) -> Self {
        Self::BuildpackError(value)
    }
}

pub(crate) fn on_error(error: libcnb::Error<AptBuildpackError>) {
    let logger = BuildLog::new(stdout()).without_buildpack_name();
    match error {
        libcnb::Error::BuildpackError(buildpack_error) => {
            on_buildpack_error(buildpack_error, logger);
        }
        framework_error => on_framework_error(&framework_error, logger),
    };
}

fn on_buildpack_error(error: AptBuildpackError, logger: Box<dyn StartedLogger>) {
    match error {
        AptBuildpackError::DetectAptfile(error) => on_detect_aptfile_error(logger, &error),
        AptBuildpackError::ReadAptfile(error) => on_read_aptfile_error(logger, &error),
        AptBuildpackError::ParseAptfile(error) => on_parse_aptfile_error(logger, &error),
        AptBuildpackError::ParseDebianArchitectureName(error) => {
            on_parse_debian_architecture_name_error(logger, &error);
        }
        AptBuildpackError::CreateAptDir(error) => on_create_apt_dir_error(logger, &error),
        AptBuildpackError::CreateAptConfig(error) => on_create_apt_config_error(logger, &error),
        AptBuildpackError::AptGetVersionCommand(error) => {
            on_apt_get_version_command_error(logger, &error);
        }
        AptBuildpackError::ParseAptGetVersion(error) => {
            on_parse_apt_get_version_error(logger, &error);
        }
        AptBuildpackError::AptGetUpdate(error) => on_apt_get_update_error(logger, &error),
        AptBuildpackError::DownloadPackages(error) => {
            on_download_packages_error(logger, &error);
        }
        AptBuildpackError::ListDownloadedPackages(error) => {
            on_list_downloaded_packages_error(logger, &error);
        }
        AptBuildpackError::InstallPackage(archive, error) => {
            on_install_package_error(logger, &archive, &error);
        }
        AptBuildpackError::ReadPackageConfigFile(config_file, error) => {
            on_read_package_config_file_error(logger, &config_file, &error);
        }
        AptBuildpackError::WritePackageConfigFile(config_file, error) => {
            on_write_package_config_file_error(logger, &config_file, &error);
        }
    };
}

fn on_detect_aptfile_error(logger: Box<dyn StartedLogger>, error: &std::io::Error) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Unable to complete buildpack detection.

            An unexpected error occurred while determining if the {buildpack_name} should be \
            run for this application. See the log output above for more information.
        ", buildpack_name = fmt::value(BUILDPACK_NAME) });
}

fn on_read_aptfile_error(logger: Box<dyn StartedLogger>, error: &std::io::Error) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Error reading {aptfile}.

            This buildpack requires an {aptfile} to complete the build but the file can't be read.

            {USE_DEBUG_INFORMATION_AND_RETRY_BUILD}

            {SUBMIT_AN_ISSUE}
        ", aptfile = fmt::value("Aptfile")});
}

fn on_parse_aptfile_error(logger: Box<dyn StartedLogger>, error: &ParseAptfileError) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Error reading {aptfile}.

            This buildpack requires an {aptfile} to complete the build but the file \
            can't be parsed.

            {USE_DEBUG_INFORMATION_AND_RETRY_BUILD}

            {SUBMIT_AN_ISSUE}
        ", aptfile = fmt::value("Aptfile")});
}

fn on_parse_debian_architecture_name_error(
    logger: Box<dyn StartedLogger>,
    error: &ParseDebianArchitectureNameError,
) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Unsupported architecture.

            The {buildpack_name} is only compatible with the following architectures:
            - amd64
        ", buildpack_name = fmt::value(BUILDPACK_NAME) });
}

fn on_create_apt_dir_error(logger: Box<dyn StartedLogger>, error: &std::io::Error) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to create {apt} download directory.

            An unexpected error occurred while creating the temporary directory for \
            downloading packages.

            {SUBMIT_AN_ISSUE}
        ", apt = fmt::value("apt") });
}

fn on_create_apt_config_error(logger: Box<dyn StartedLogger>, error: &std::io::Error) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to write {apt_configuration_file} configuration.

            An unexpected error occurred while configuring {apt} to use a temporary directory \
            for package downloads.

            {SUBMIT_AN_ISSUE}
        ", apt = fmt::value("apt"), apt_configuration_file = fmt::value("apt.conf") });
}

fn on_apt_get_version_command_error(logger: Box<dyn StartedLogger>, error: &fun_run::CmdError) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to determine {apt} version information.

            An unexpected error occurred while executing {apt_version}.

            {SUBMIT_AN_ISSUE}
        ", apt = fmt::value("apt"), apt_version = fmt::command(error.name()) });
}

fn on_parse_apt_get_version_error(logger: Box<dyn StartedLogger>, error: &ParseAptVersionError) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to parse {apt} version information.

            An unexpected error occurred while parsing version information.

            {SUBMIT_AN_ISSUE}
        ", apt = fmt::value("apt") });
}

fn on_apt_get_update_error(logger: Box<dyn StartedLogger>, error: &fun_run::CmdError) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to update {apt} package indexes.

            Before installing packages, the {buildpack_name} updates the package index files used \
            by {apt}. These index files contain information about available packages and versions. \
            This update ensures the latest package information will be used.

            An unexpected error occurred while executing the update command {apt_get_update} and the \
            buildpack cannot continue. See the log output above for more information.

            This error can be caused by an unstable network connection. Check the status of archive.ubuntu.com \
            at https://status.canonical.com/ and retry your build.
        ", apt = fmt::value("apt"), apt_get_update = fmt::command(error.name()), buildpack_name = fmt::value(BUILDPACK_NAME) });
}

fn on_download_packages_error(logger: Box<dyn StartedLogger>, error: &fun_run::CmdError) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to download packages.

            The {buildpack_name} uses the command {apt_get_install} to download packages. This \
            command failed and the buildpack cannot continue. See the log output above for more information.

            This error can be caused by an unstable network connection or an unavailable/misspelled package \
            name. Check the status of archive.ubuntu.com at https://status.canonical.com/, verify \
            package availability at https://packages.ubuntu.com/, and retry your build.
        ", apt_get_install = fmt::command(error.name()), buildpack_name = fmt::value(BUILDPACK_NAME) });
}

fn on_list_downloaded_packages_error(logger: Box<dyn StartedLogger>, error: &std::io::Error) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to list downloaded packages.

            An unexpected error occurred while reading the directory containing the downloaded \
            packages. See the log output above for more information.

            {SUBMIT_AN_ISSUE}
        " });
}

fn on_install_package_error(
    logger: Box<dyn StartedLogger>,
    archive: &Path,
    error: &fun_run::CmdError,
) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to install package from {archive}.

            The {buildpack_name} uses the command {dpkg_extract} to install the package. This command \
            failed and the buildpack cannot continue. See the log output above for more information.

            {SUBMIT_AN_ISSUE}
        ", archive = archive.display(), buildpack_name = fmt::value(BUILDPACK_NAME), dpkg_extract = fmt::command(error.name()) });
}

fn on_read_package_config_file_error(
    logger: Box<dyn StartedLogger>,
    config_file: &Path,
    error: &std::io::Error,
) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Could not read pkg-config metadata.

            An unexpected error occurred while reading the pkg-config metadata file at {config_file}. \
            See the log output above for more information.

            {SUBMIT_AN_ISSUE}
        ", config_file = config_file.display() });
}

fn on_write_package_config_file_error(
    logger: Box<dyn StartedLogger>,
    config_file: &Path,
    error: &std::io::Error,
) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! { "
            Failed to update pkg-config metadata.

            The {buildpack_name} modifies the pkg-config metadata of installed packages to ensure that \
            software dependencies can be compiled with the correct library path information. An unexpected \
            error occurred while writing {config_file} and the buildpack cannot continue. See the log \
            output above for more information.

            {SUBMIT_AN_ISSUE}
        ", config_file = config_file.display(), buildpack_name = fmt::value(BUILDPACK_NAME) });
}

fn on_framework_error(error: &libcnb::Error<AptBuildpackError>, logger: Box<dyn StartedLogger>) {
    print_error_details(logger, &error)
        .announce()
        .error(&formatdoc! {"
            {buildpack_name} internal error.

            The framework used by this buildpack encountered an unexpected error.

            If you can't deploy to Heroku due to this issue, check the official Heroku Status page at \
            status.heroku.com for any ongoing incidents. After all incidents resolve, retry your build.

            {SUBMIT_AN_ISSUE}
        ", buildpack_name = fmt::value(BUILDPACK_NAME) });
}

fn print_error_details(
    logger: Box<dyn StartedLogger>,
    error: &impl Display,
) -> Box<dyn StartedLogger> {
    logger
        .section(DEBUG_INFO)
        .step(&error.to_string())
        .end_section()
}

const USE_DEBUG_INFORMATION_AND_RETRY_BUILD: &str = "\
Use the debug information above to troubleshoot and retry your build.";

const SUBMIT_AN_ISSUE: &str = "\
If the issue persists and you think you found a bug in the buildpack then reproduce the issue \
locally with a minimal example and open an issue in the buildpack's GitHub repository with the details.";
