use crate::config::{ConfigError, ParseConfigError, ParseRequestedPackageError};
use crate::create_package_index::CreatePackageIndexError;
use crate::debian::UnsupportedDistroError;
use crate::determine_packages_to_install::DeterminePackagesToInstallError;
use crate::errors::ErrorType::{Framework, Internal, UserFacing};
use crate::install_packages::InstallPackagesError;
use crate::DebianPackagesBuildpackError;
use std::collections::BTreeSet;

use bon::builder;
use bullet_stream::{style, Print};
use indoc::{formatdoc, indoc};
use libcnb::Error;
use std::io::Write;
use std::path::Path;

const BUILDPACK_NAME: &str = "Heroku Deb Packages buildpack";

pub(crate) fn on_error<W>(error: Error<DebianPackagesBuildpackError>, writer: W)
where
    W: Write + Sync + Send + 'static,
{
    print_error(
        match error {
            Error::BuildpackError(e) => on_buildpack_error(e),
            e => on_framework_error(&e),
        },
        writer,
    );
}

fn on_buildpack_error(error: DebianPackagesBuildpackError) -> ErrorMessage {
    match error {
        DebianPackagesBuildpackError::Config(e) => on_config_error(e),
        DebianPackagesBuildpackError::UnsupportedDistro(e) => on_unsupported_distro_error(e),
        DebianPackagesBuildpackError::CreatePackageIndex(e) => on_create_package_index_error(e),
        DebianPackagesBuildpackError::DeterminePackagesToInstall(e) => {
            on_determine_packages_to_install_error(e)
        }
        DebianPackagesBuildpackError::InstallPackages(e) => on_install_packages_error(e),
    }
}

#[allow(clippy::too_many_lines)]
fn on_config_error(error: ConfigError) -> ErrorMessage {
    match error {
        ConfigError::CheckExists(config_file, e) => {
            let config_file = file_value(config_file);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::No, SuggestSubmitIssue::No))
                .header("Unable to complete buildpack detection")
                .body(formatdoc! { "
                    An unexpected I/O error occurred while checking {config_file} to determine if the \
                    {BUILDPACK_NAME} is compatible for this application.
                " })
                .debug_info(e.to_string())
                .call()
        }

        ConfigError::ReadConfig(config_file, e) => {
            let config_file = file_value(config_file);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                .header(format!("Error reading {config_file}"))
                .body(formatdoc! { "
                    The {BUILDPACK_NAME} reads configuration from {config_file} to complete the build but \
                    the file can't be read.

                    Suggestions:
                    - Ensure the file has read permissions.
                " })
                .debug_info(e.to_string())
                .call()
        }

        ConfigError::ParseConfig(config_file, error) => {
            let config_file = file_value(config_file);
            let toml_spec_url = style::url("https://toml.io/en/v1.0.0");
            let root_config_key = style::value("[com.heroku.buildpacks.debian-packages]");
            let configuration_doc_url = style::url("https://github.com/heroku/buildpacks-debian-packages?tab=readme-ov-file#configuration");
            let debian_package_name_format_url = style::url(
                "https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-source",
            );
            let package_search_url = get_package_search_url();

            match error {
                ParseConfigError::InvalidToml(error) => {
                    create_error()
                        .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                        .header(format!("Error parsing {config_file} with invalid TOML file"))
                        .body(formatdoc! { "
                            The {BUILDPACK_NAME} reads configuration from {config_file} to complete the build but \
                            this file isn't a valid TOML file.

                            Suggestions:
                            - Ensure the file follows the TOML format described at {toml_spec_url}
                        " })
                        .debug_info(error.to_string())
                        .call()
                }

                ParseConfigError::WrongConfigType => {
                    create_error()
                        .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                        .header(format!("Error parsing {config_file} with invalid key"))
                        .body(formatdoc! { "
                            The {BUILDPACK_NAME} reads the configuration from {config_file} to complete \
                            the build but the configuration for the key {root_config_key} isn't the \
                            correct type. The value of this key must be a TOML table.

                            Suggestions:
                            - See the buildpack documentation for the proper usage for this configuration at \
                            {configuration_doc_url}
                            - See the TOML documentation for more details on the TOML table type at \
                            {toml_spec_url}
                        " })
                        .call()
                }

                ParseConfigError::ParseRequestedPackage(error) => match error {
                    ParseRequestedPackageError::InvalidPackageName(error) => {
                        let invalid_package_name = style::value(error.package_name);

                        create_error()
                            .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                            .header(format!("Error parsing {config_file} with invalid package name"))
                            .body(formatdoc! { "
                                The {BUILDPACK_NAME} reads configuration from {config_file} to \
                                complete the build but we found an invalid package name {invalid_package_name} \
                                in the key {root_config_key}.

                                Package names must consist only of lowercase letters (a-z), \
                                digits (0-9), plus (+) and minus (-) signs, and periods (.). Names \
                                must be at least two characters long and must start with an alphanumeric \
                                character. See {debian_package_name_format_url}

                                Suggestions:
                                - Verify the package name is correct and exists for the target distribution at \
                                 {package_search_url}
                            " })
                            .call()
                    }

                    ParseRequestedPackageError::UnexpectedTomlValue(value) => {
                        let string_example = "\"package-name\"";
                        let inline_table_example =
                            r#"{ name = "package-name", skip_dependencies = true }"#;
                        let value_type = style::value(value.type_name());
                        let value = style::value(value.to_string());

                        create_error()
                            .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                            .header(format!("Error parsing {config_file} with invalid package format"))
                            .body(formatdoc! { "
                                The {BUILDPACK_NAME} reads configuration from {config_file} to \
                                complete the build but we found an invalid package format in the \
                                key {root_config_key}.

                                Packages must either be the following TOML values:
                                - String (e.g.; {string_example})
                                - Inline table (e.g.; {inline_table_example})

                                Suggestions:
                                - See the buildpack documentation for the proper usage for this configuration at \
                                {configuration_doc_url}
                                - See the TOML documentation for more details on the TOML string \
                                and inline table types at {toml_spec_url}
                            " })
                            .debug_info(format!("Invalid type {value_type} with value {value}"))
                            .call()
                    }
                },
            }
        }
    }
}

fn on_unsupported_distro_error(error: UnsupportedDistroError) -> ErrorMessage {
    let UnsupportedDistroError {
        name,
        version,
        architecture,
    } = error;

    create_error()
        .error_type(Internal)
        .header("Unsupported distribution")
        .body(formatdoc! { "
            The {BUILDPACK_NAME} doesn't support the {name} {version} ({architecture}) distribution.
        " })
        .call()
}

#[allow(clippy::too_many_lines)]
fn on_create_package_index_error(error: CreatePackageIndexError) -> ErrorMessage {
    let canonical_status_url = get_canonical_status_url();

    match error {
        CreatePackageIndexError::NoSources => {
            create_error()
                .error_type(Internal)
                .header("No sources to update")
                .body(indoc! { "
                    The distribution has no sources to update packages from.
                " })
                .call()
        }

        CreatePackageIndexError::TaskFailed(e) => {
            create_error()
                .error_type(Internal)
                .header("Task failure while updating sources")
                .body(indoc! { "
                    A background task responsible for updating sources failed to complete.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::InvalidLayerName(url, e) => {
            create_error()
                .error_type(Internal)
                .header("Invalid layer name")
                .body(formatdoc! { "
                    For caching purposes, a unique layer name is generated for Debian Release files \
                    and Package indices based on their download urls. The generated name for the \
                    following url was invalid:
                    - {url}

                    You can find the invalid layer name in the debug information above.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::GetReleaseRequest(e) => {
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to request Release file")
                .body(formatdoc! { "
                    While updating package sources, a request to download a Release file failed. \
                    This error can occur due to an unstable network connection or an issue with the upstream \
                    Debian package repository.

                    Suggestions:
                    - Check the status of {canonical_status_url} for any reported issues.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::ReadGetReleaseResponse(e) => {
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to download Release file")
                .body(formatdoc! { "
                    While updating package sources, an error occurred while downloading a Release file. \
                    This error can occur due to an unstable network connection or an issue with the upstream \
                    Debian package repository.

                    Suggestions:
                    - Check the status of {canonical_status_url} for any reported issues.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::CreatePgpCertificate(e) => {
            create_error()
                .error_type(Internal)
                .header("Failed to load verifying PGP certificate")
                .body(indoc! { "
                    The PGP certificate used to verify downloaded release files failed to load. This \
                    error indicates there's a problem with the format of the certificate file the \
                    distribution uses.

                    Suggestions:
                    - Verify the format of the certificates found in the ./keys directory of this \
                    buildpack's repository. See https://cirw.in/gpg-decoder
                    - Extract new certificates by running the ./scripts/extract_keys.sh script found \
                    in this buildpack's repository.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::CreatePgpVerifier(e) => {
            create_error()
                .error_type(Internal)
                .header("Failed to verify Release file")
                .body(indoc! { "
                    The PGP signature of the downloaded release file failed verification. This error can \
                    occur if the maintainers of the Debian repository changed the process for signing \
                    release files.

                    Suggestions:
                    - Verify if the keys changed by running the ./scripts/extract_keys.sh \
                    script found in this buildpack's repository.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::WriteReleaseLayer(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to write Release file to layer")
                .body(formatdoc! { "
                    An unexpected I/O error occurred while writing release data to {file}.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::ReadReleaseFile(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to read Release file from layer")
                .body(formatdoc! { "
                    An unexpected I/O error occurred while reading Release data from {file}.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::ParseReleaseFile(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to parse Release file data")
                .body(formatdoc! { "
                    We couldn't parse the Release file data stored in {file}. This error is most likely \
                    a buildpack bug. It can also be caused by cached data that's no longer valid or an \
                    issue with the upstream repository.

                    Suggestions:
                    - Run the build again with a clean cache.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::MissingSha256ReleaseHashes(release_uri) => {
            let release_uri = style::url(release_uri.as_str());
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Missing SHA256 Release hash")
                .body(formatdoc! { "
                    The Release file from {release_uri} is missing the SHA256 key which is required \
                    according to the documented Debian repository format. This error is most likely an issue \
                    with the upstream repository. See https://wiki.debian.org/DebianRepository/Format
                " })
                .call()
        }

        CreatePackageIndexError::MissingPackageIndexReleaseHash(release_uri, package_index) => {
            let release_uri = style::url(release_uri.as_str());
            let package_index = style::value(package_index);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Missing Package Index")
                .body(formatdoc! { "
                    The Release file from {release_uri} is missing an entry for {package_index} within \
                    the SHA256 section. This error is most likely a buildpack bug but can also \
                    be an issue with the upstream repository.

                    Suggestions:
                    - Verify if {package_index} is under the SHA256 section of {release_uri}
                " })
                .call()
        }

        CreatePackageIndexError::GetPackagesRequest(e) => {
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to request Package Index file")
                .body(formatdoc! { "
                    While updating package sources, a request to download a Package Index file failed. \
                    This error can occur due to an unstable network connection or an issue with the upstream \
                    Debian package repository.

                    Suggestions:
                    - Check the status of {canonical_status_url} for any reported issues.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::WritePackagesLayer(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to write Package Index file to layer")
                .body(formatdoc! { "
                    An unexpected I/O error occurred while writing Package Index data to {file}.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::WritePackageIndexFromResponse(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to download Package Index file")
                .body(formatdoc! { "
                    While updating package sources, an error occurred while downloading a Package Index \
                    file to {file}. This error can occur due to an unstable network connection or an issue \
                    with the upstream Debian package repository.

                    Suggestions:
                    - Check the status of {canonical_status_url} for any reported issues.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::ChecksumFailed {
            url,
            expected,
            actual,
        } => {
            let url = style::url(url);
            let expected = style::value(expected);
            let actual = style::value(actual);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                .header("Package Index checksum verification failed")
                .body(formatdoc! { "
                    While updating package sources, an error occurred while verifying the checksum \
                    of the Package Index at {url}. This error can occur due to an issue with the upstream \
                    Debian package repository.

                    Checksum:
                    - Expected: {expected}
                    - Actual: {actual}
                " })
                .call()
        }

        CreatePackageIndexError::CpuTaskFailed(e) => {
            create_error()
                .error_type(Internal)
                .header("Task failure while reading Package Index data")
                .body(indoc! { "
                    A background task responsible for reading Package Index data failed to complete.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::ReadPackagesFile(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to read Package Index file")
                .body(formatdoc! { "
                    An unexpected I/O error occurred while reading Package Index data from {file}.
                " })
                .debug_info(e.to_string())
                .call()
        }

        CreatePackageIndexError::ParsePackages(file, errors) => {
            let file = file_value(file);
            let body_start = formatdoc! { "
                We can't parse the Package Index file data stored in {file}. This error is most likely \
                a buildpack bug. It can also be caused by cached data that's no longer valid or an issue \
                with the upstream repository.

                Parsing errors:
            " }.trim_end().to_string();
            let body_error_details = errors
                .iter()
                .map(|e| format!("- {e}"))
                .collect::<Vec<_>>()
                .join("\n");
            let body_end = indoc! { "
                Suggestions:
                - Run the build again with a clean cache
            " };
            create_error()
                .error_type(Internal)
                .header("Failed to parse Package Index file")
                .body(format!(
                    "{body_start}\n{body_error_details}\n\n{body_end}"
                ))
                .call()
        }
    }
}

fn on_determine_packages_to_install_error(error: DeterminePackagesToInstallError) -> ErrorMessage {
    match error {
        DeterminePackagesToInstallError::ReadSystemPackages(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to read system packages")
                .body(formatdoc! { "
                    An unexpected I/O error occurred while reading system packages from {file}.
                "})
                .debug_info(e.to_string())
                .call()
        }

        DeterminePackagesToInstallError::ParseSystemPackage(file, package_data, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to parse system package")
                .body(formatdoc! { "
                    An unexpected parsing error occurred while reading system packages from {file}.
                "})
                .debug_info(format!("{e}\n\nPackage:\n{package_data}"))
                .call()
        }

        DeterminePackagesToInstallError::PackageNotFound(package_name) => {
            let package_name = style::value(package_name);
            let package_search_url = get_package_search_url();
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                .header("Package not found")
                .body(formatdoc! { "
                    We can't find {package_name} in the Package Index. If this package is listed in the \
                    packages to install for this buildpack then the name is most likely misspelled. Otherwise, \
                    it can be an issue with the upstream Debian package repository.

                    Suggestions:
                    - Verify the package name is correct and exists for the target distribution at \
                    {package_search_url}
                " })
                .call()
        }

        DeterminePackagesToInstallError::VirtualPackageMustBeSpecified(package, providers) => {
            let package = style::value(package);
            let body_start = indoc! { "
                Sometimes there are several packages which offer more-or-less the same functionality. \
                In this case, Debian repositories define a virtual package and one or more actual \
                packages provide an implementation for this virtual package. When multiple providers \
                are found for a requested package, this buildpack can't automatically choose which \
                one is the desired implementation.

                Providing packages:
            " };
            let body_provider_details = providers
                .iter()
                .collect::<BTreeSet<_>>()
                .iter()
                .map(|provider| format!("- {provider}"))
                .collect::<Vec<_>>()
                .join("\n");
            let body_end = formatdoc! { "
                Suggestions:
                - Replace the virtual package {package} with one of the above providers
            " };
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                .header(format!(
                    "Multiple providers were found for the package {package}"
                ))
                .body(format!("{body_start}{body_provider_details}\n\n{body_end}"))
                .call()
        }
    }
}

#[allow(clippy::too_many_lines)]
fn on_install_packages_error(error: InstallPackagesError) -> ErrorMessage {
    let canonical_status_url = get_canonical_status_url();

    match error {
        InstallPackagesError::TaskFailed(e) => create_error()
            .error_type(Internal)
            .header("Task failure while installing packages")
            .body(indoc! { "
                A background task responsible for installing failed to complete.
            " })
            .debug_info(e.to_string())
            .call(),

        InstallPackagesError::InvalidFilename(package, filename) => {
            let package = style::value(package);
            let filename = style::value(filename);
            create_error()
                .error_type(Internal)
                .header(format!("Could not determine file name for {package}"))
                .body(formatdoc! { "
                    The package information for {package} contains a Filename field of {filename} \
                    which produces an invalid name to use as a download path.
                " })
                .call()
        }

        InstallPackagesError::RequestPackage(package, e) => {
            let package = style::value(package);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to request package")
                .body(formatdoc! { "
                    While installing packages, an error occurred while downloading {package}. \
                    This error can occur due to an unstable network connection or an issue \
                    with the upstream Debian package repository.

                    Suggestions:
                    - Check the status of {canonical_status_url} for any reported issues.
                " })
                .debug_info(e.to_string())
                .call()
        }

        InstallPackagesError::WritePackage(package, download_url, destination_path, e) => {
            let package = style::value(package);
            let download_url = style::url(download_url);
            let destination_path = file_value(destination_path);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to download package")
                .body(formatdoc! { "
                    An unexpected I/O error occured while downloading {package} from {download_url} \
                    to {destination_path}. This error can occur due to an unstable network connection or an issue \
                    with the upstream Debian package repository.

                    Suggestions:
                    - Check the status of {canonical_status_url} for any reported issues.
                " })
                .debug_info(e.to_string())
                .call()
        }

        InstallPackagesError::ChecksumFailed {
            url,
            expected,
            actual,
        } => {
            let url = style::url(url);
            let expected = style::value(expected);
            let actual = style::value(actual);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                .header("Package checksum verification failed")
                .body(formatdoc! { "
                    An error occurred while verifying the checksum of the package at {url}. \
                    This error can occur due to an issue with the upstream Debian package repository.

                    Checksum:
                    - Expected: {expected}
                    - Actual: {actual}
                " })
                .call()
        }

        InstallPackagesError::OpenPackageArchive(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to open package archive")
                .body(formatdoc! {
                    "An unexpected I/O error occurred while trying to open the archive at {file}."
                })
                .debug_info(e.to_string())
                .call()
        }

        InstallPackagesError::OpenPackageArchiveEntry(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to read package archive entry")
                .body(formatdoc! {
                    "An unexpected I/O error occurred while trying to read the entries of the \
                    archive at {file}."
                })
                .debug_info(e.to_string())
                .call()
        }

        InstallPackagesError::UnpackTarball(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to unpack package archive")
                .body(formatdoc! {
                    "An unexpected I/O error occurred while trying to unpack the archive at {file}."
                })
                .debug_info(e.to_string())
                .call()
        }

        InstallPackagesError::UnsupportedCompression(file, format) => {
            let file = file_value(file);
            let format = style::value(format);
            create_error()
                .error_type(Internal)
                .header("Unsupported compression format for package archive")
                .body(formatdoc! {
                    "An unexpected compression format ({format}) was used for the package archive at {file}."
                })
                .call()
        }

        InstallPackagesError::ReadPackageConfig(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to read package config file")
                .body(formatdoc! {
                    "An unexpected I/O error occurred while reading the package config file at {file}."
                })
                .debug_info(e.to_string())
                .call()
        }

        InstallPackagesError::WritePackageConfig(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(Internal)
                .header("Failed to write package config file")
                .body(formatdoc! {
                    "An unexpected I/O error occurred while writing the package config file to {file}."
                })
                .debug_info(e.to_string())
                .call()
        }
    }
}

fn on_framework_error(error: &Error<DebianPackagesBuildpackError>) -> ErrorMessage {
    create_error()
        .error_type(Framework)
        .header("Heroku Deb Packages Buildpack internal error")
        .body(formatdoc! {"
            The framework used by this buildpack encountered an unexpected error.

            If you can’t deploy to Heroku due to this issue, check the official Heroku Status page at \
            status.heroku.com for any ongoing incidents. After all incidents resolve, retry your build.

            Use the debug information above to troubleshoot and retry your build. If you think you found a \
            bug in the buildpack, reproduce the issue locally with a minimal example and file an issue here:
            https://github.com/heroku/buildpacks-debian-packages/issues/new
        "})
        .debug_info(error.to_string())
        .call()
}

#[builder]
fn create_error(
    header: impl AsRef<str>,
    body: impl AsRef<str>,
    error_type: ErrorType,
    debug_info: Option<String>,
) -> ErrorMessage {
    let mut message_parts = vec![
        header.as_ref().trim().to_string(),
        body.as_ref().trim().to_string(),
    ];
    let issues_url = style::url("https://github.com/heroku/buildpacks-debian-packages/issues/new");

    match error_type {
        Framework => {}
        Internal => {
            message_parts.push(formatdoc! { "
                This error is almost always a buildpack bug. If you see this error, please file an \
                issue here:
                {issues_url}
            "});
        }
        UserFacing(suggest_retry_build, suggest_submit_issue) => {
            if let SuggestRetryBuild::Yes = suggest_retry_build {
                message_parts.push(
                    formatdoc! { "
                    Use the debug information above to troubleshoot and retry your build.
                "}
                    .trim()
                    .to_string(),
                );
            }

            if let SuggestSubmitIssue::Yes = suggest_submit_issue {
                message_parts.push(formatdoc! { "
                    If the issue persists and you think you found a bug in the buildpack, reproduce \
                    the issue locally with a minimal example. Open an issue in the buildpack's GitHub \
                    repository and include the details here:
                    {issues_url}
                "}.trim().to_string());
            }
        }
    }

    let message = message_parts.join("\n\n");

    ErrorMessage {
        debug_info,
        message,
    }
}

fn print_error<W>(error_message: ErrorMessage, writer: W)
where
    W: Write + Send + Sync + 'static,
{
    let mut log = Print::new(writer).without_header();
    if let Some(debug_info) = error_message.debug_info {
        log = log
            .bullet(style::important("Debug Info:"))
            .sub_bullet(debug_info)
            .done();
    }
    log.error(error_message.message);
}

fn file_value(value: impl AsRef<Path>) -> String {
    style::value(value.as_ref().to_string_lossy())
}

fn get_canonical_status_url() -> String {
    style::url("https://status.canonical.com/")
}

fn get_package_search_url() -> String {
    style::url("https://packages.ubuntu.com/")
}

#[derive(Debug)]
struct ErrorMessage {
    debug_info: Option<String>,
    message: String,
}

#[derive(Debug, PartialEq)]
enum ErrorType {
    Framework,
    Internal,
    UserFacing(SuggestRetryBuild, SuggestSubmitIssue),
}

#[derive(Debug, PartialEq)]
enum SuggestRetryBuild {
    Yes,
    No,
}

#[derive(Debug, PartialEq)]
enum SuggestSubmitIssue {
    Yes,
    No,
}
