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

const BUILDPACK_NAME: &str = "Heroku .deb Packages buildpack";

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
            let root_config_key = style::value("[com.heroku.buildpacks.deb-packages]");
            let configuration_doc_url =
                style::url("https://github.com/heroku/buildpacks-deb-packages#configuration");
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

            Supported distributions:
            - Ubuntu 24.04 (amd64, arm64)
            - Ubuntu 22.04 (amd64)
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
                - Run the build again with a clean cache.
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

        DeterminePackagesToInstallError::PackageNotFound(package_name, suggested_packages) => {
            let package_name = style::value(package_name);
            let package_search_url = get_package_search_url();
            let suggestions = if suggested_packages.is_empty() {
                "- No similarly named packages found".to_string()
            } else {
                suggested_packages
                    .into_iter()
                    .map(|name| format!("- {}", style::value(name)))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                .header("Package not found")
                .body(formatdoc! { "
                    We can't find {package_name} in the Package Index. If this package is listed in the \
                    packages to install for this buildpack then the name is most likely misspelled. Otherwise, \
                    it can be an issue with the upstream Debian package repository.

                    Did you mean?
                    {suggestions}

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
                - Replace the virtual package {package} with one of the above providers.
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
            let package = style::value(package.name);
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
            let package = style::value(package.name);
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
            https://github.com/heroku/buildpacks-deb-packages/issues/new
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
    let issues_url = style::url("https://github.com/heroku/buildpacks-deb-packages/issues/new");
    let pack = style::value("pack");
    let pack_url =
        style::url("https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/");

    match error_type {
        Framework => {}
        Internal => {
            message_parts.push(formatdoc! { "
                The causes for this error are unknown. We do not have suggestions for diagnosis or a \
                workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                {issues_url}

                If you're able to reproduce the problem with an example application and the {pack} \
                build tool ({pack_url}), adding that information to the discussion will also help. Once \
                we have more information around the causes of this error we may update this message.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debian::{
        ParsePackageNameError, ParseRepositoryPackageError, RepositoryPackage, RepositoryUri,
    };
    use crate::DebianPackagesBuildpackError::UnsupportedDistro;
    use anyhow::anyhow;
    use libcnb::data::layer::LayerNameError;
    use libcnb_test::assert_contains_match;
    use std::collections::HashSet;
    use std::str::FromStr;

    #[test]
    fn test_config_check_exists_errors() {
        test_error_output(
            "
                Context
                -------
                When detect is executed on this buildpack, we check to see if a project.toml
                file exists since that's where configuration for this buildpack would be found.
                I/O operations can fail for a number of reasons which we can't anticipate and
                the best we can do here is report the error message.
            ",
            ConfigError::CheckExists(
                "/path/to/project.toml".into(),
                create_io_error("test I/O error"),
            ),
            indoc! {"
                - Debug Info:
                  - test I/O error

                ! Unable to complete buildpack detection
                !
                ! An unexpected I/O error occurred while checking `/path/to/project.toml` to \
                determine if the Heroku .deb Packages buildpack is compatible for this application.
            "},
        );
    }

    #[test]
    fn config_read_config_error() {
        test_error_output(
            "
                Context
                -------
                We read the buildpack configuration from project.toml. I/O operations can fail
                for a number of reasons which we can't anticipate but the most likely one here would
                be that we don't have read permissions.
            ",
            ConfigError::ReadConfig(
                "/path/to/project.toml".into(),
                create_io_error("test I/O error"),
            ),
            indoc! {"
                - Debug Info:
                  - test I/O error

                ! Error reading `/path/to/project.toml`
                !
                ! The Heroku .deb Packages buildpack reads configuration from `/path/to/project.toml` \
                to complete the build but the file can't be read.
                !
                ! Suggestions:
                ! - Ensure the file has read permissions.
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn config_parse_config_error_for_wrong_config_type() {
        test_error_output("
                Context
                -------
                We read the buildpack configuration from project.toml which must be a valid TOML file.
                If the file is valid but the value supplied using the configuration key for this buildpack
                is not a TOML table then the configuration is incorrect and we can provide details to the
                user on how they can fix this.
            ",
            ConfigError::ParseConfig(
                "/path/to/project.toml".into(),
                ParseConfigError::WrongConfigType,
            ),
            indoc! {"
                ! Error parsing `/path/to/project.toml` with invalid key
                !
                ! The Heroku .deb Packages buildpack reads the configuration from `/path/to/project.toml` \
                to complete the build but the configuration for the key `[com.heroku.buildpacks.deb-packages]` \
                isn't the correct type. The value of this key must be a TOML table.
                !
                ! Suggestions:
                ! - See the buildpack documentation for the proper usage for this configuration at \
                https://github.com/heroku/buildpacks-deb-packages#configuration
                ! - See the TOML documentation for more details on the TOML table type at \
                https://toml.io/en/v1.0.0
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn config_parse_config_error_for_invalid_toml() {
        test_error_output("
                Context
                -------
                We read the buildpack configuration from project.toml which must be a valid TOML file.
                This error will be reported if the file is not valid TOML.
            ",
            ConfigError::ParseConfig(
                "/path/to/project.toml".into(),
                ParseConfigError::InvalidToml(
                    toml_edit::DocumentMut::from_str("[com.heroku").unwrap_err(),
                ),
            ),
            indoc! {"
                - Debug Info:
                  - TOML parse error at line 1, column 12
                      |
                    1 | [com.heroku
                      |            ^
                    invalid table header
                    expected `.`, `]`

                ! Error parsing `/path/to/project.toml` with invalid TOML file
                !
                ! The Heroku .deb Packages buildpack reads configuration from `/path/to/project.toml` \
                to complete the build but this file isn't a valid TOML file.
                !
                ! Suggestions:
                ! - Ensure the file follows the TOML format described at https://toml.io/en/v1.0.0
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn config_parse_config_error_for_invalid_package_name() {
        test_error_output("
                Context
                -------
                We read the buildpack configuration from project.toml which must be a valid TOML file.
                If the file is valid but the value supplied using the configuration for this buildpack
                contains a package name that would be invalid according to Debian package naming policies,
                we report this package name to the user and ask them to verify it.
            ",
            ConfigError::ParseConfig(
                "/path/to/project.toml".into(),
                ParseConfigError::ParseRequestedPackage(
                    ParseRequestedPackageError::InvalidPackageName(ParsePackageNameError {
                        package_name: "invalid!package!name".to_string(),
                    }),
                ),
            ),
            indoc! {"
                ! Error parsing `/path/to/project.toml` with invalid package name
                !
                ! The Heroku .deb Packages buildpack reads configuration from `/path/to/project.toml` \
                to complete the build but we found an invalid package name `invalid!package!name` \
                in the key `[com.heroku.buildpacks.deb-packages]`.
                !
                ! Package names must consist only of lowercase letters (a-z), digits (0-9), plus (+) \
                and minus (-) signs, and periods (.). Names must be at least two characters long and \
                must start with an alphanumeric character. \
                See https://www.debian.org/doc/debian-policy/ch-controlfields.html#s-f-source
                !
                ! Suggestions:
                ! - Verify the package name is correct and exists for the target distribution at https://packages.ubuntu.com/
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn config_parse_config_error_for_invalid_package_name_config_type() {
        test_error_output("
                Context
                -------
                We read the buildpack configuration from project.toml which must be a valid TOML file.
                If the file is valid but the value supplied using the configuration for this buildpack
                contains a package entry that is neither a string nor inline table value, it is invalid.
                We report this to the user and request they check the buildpack documentation for proper
                usage.
            ",
            ConfigError::ParseConfig(
                "/path/to/project.toml".into(),
                ParseConfigError::ParseRequestedPackage(
                    ParseRequestedPackageError::UnexpectedTomlValue(
                        toml_edit::value(37).into_value().unwrap(),
                    ),
                ),
            ),
            indoc! {"
                - Debug Info:
                  - Invalid type `integer` with value `37`

                ! Error parsing `/path/to/project.toml` with invalid package format
                !
                ! The Heroku .deb Packages buildpack reads configuration from `/path/to/project.toml` \
                to complete the build but we found an invalid package format in the key \
                `[com.heroku.buildpacks.deb-packages]`.
                !
                ! Packages must either be the following TOML values:
                ! - String (e.g.; \"package-name\")
                ! - Inline table (e.g.; { name = \"package-name\", skip_dependencies = true })
                !
                ! Suggestions:
                ! - See the buildpack documentation for the proper usage for this configuration at \
                https://github.com/heroku/buildpacks-deb-packages#configuration
                ! - See the TOML documentation for more details on the TOML string and inline \
                table types at https://toml.io/en/v1.0.0
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn unsupported_distro_error() {
        test_error_output("
                Context
                -------
                This buildpack only supports the following distributions:
                - Ubuntu 22.04 (amd64)
                - Ubuntu 24.04 (amd64, arm64)

                Anything else is unsupported. This error is unlikely to be seen by an end-user but may
                be helpful for developers hacking on this buildpack. Tools like pack also validate
                buildpacks against their target distribution metadata to prevent this exact scenario.
            ",
            UnsupportedDistro(UnsupportedDistroError {
                name: "Windows".to_string(),
                version: "XP".to_string(),
                architecture: "x86".to_string(),
            }),
            indoc! {"
                ! Unsupported distribution
                !
                ! The Heroku .deb Packages buildpack doesn't support the Windows XP (x86) distribution.
                !
                ! Supported distributions:
                ! - Ubuntu 24.04 (amd64, arm64)
                ! - Ubuntu 22.04 (amd64)
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_no_sources() {
        test_error_output("
                Context
                -------
                This is a developer error. It should never be seen by an end-user but could occur if
                the registered sources for a distribution were modified or someone forgot to register
                ones for a specific architecture. Our testing processes should always catch this but,
                if not, we should direct users to file an issue.
            ",
            CreatePackageIndexError::NoSources,
            indoc! {"
                ! No sources to update
                !
                ! The distribution has no sources to update packages from.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_task_failed() {
        test_error_output_with_custom_assertion(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. Package indexes
                are updated using async tasks which can fail if the task panics or is cancelled but
                we don't cancel running tasks and we handle all errors.
            ",
            CreatePackageIndexError::TaskFailed(create_join_error()),
            |actual_text| {
                assert_contains_match!(
                    actual_text,
                    indoc! {"
                        - Debug Info:
                          - task \\d+ panicked with message \"uh oh!\"

                        ! Task failure while updating sources
                        !
                        ! A background task responsible for updating sources failed to complete.
                        !
                        ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                        a workaround at this time. You can help our understanding by sharing your buildpack log \
                        and a description of the issue at:
                        ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                        !
                        ! If you're able to reproduce the problem with an example application and the `pack` \
                        build tool \\(https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/\\), \
                        adding that information to the discussion will also help. Once we have more information \
                        around the causes of this error we may update this message.
                    "}
                );
            },
        );
    }

    #[test]
    fn create_package_index_error_invalid_layer_name() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. The only restrictions
                on layer naming are that it can't be named 'build', 'launch', or 'cache' and it must be
                a valid filename. The hexidecimal character set used here should be safe. If a bug is
                introduced in how this name is generated then we report this and ask the user to file
                an issue against this buildpack.
            ",
            CreatePackageIndexError::InvalidLayerName(
                "http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease".to_string(),
                LayerNameError::InvalidValue(
                    "623e1a2085e65abc3dc626a97909466ce19efe37f7a6529a842c290fcfc65b3b".to_string(),
                ),
            ),
            indoc! {"
                - Debug Info:
                  - Invalid Value: 623e1a2085e65abc3dc626a97909466ce19efe37f7a6529a842c290fcfc65b3b

                ! Invalid layer name
                !
                ! For caching purposes, a unique layer name is generated for Debian Release files \
                and Package indices based on their download urls. The generated name for the following \
                url was invalid:
                ! - http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease
                !
                ! You can find the invalid layer name in the debug information above.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_get_release_request() {
        test_error_output(
            "
                Context
                -------
                Package sources are requested from a Debian repository starting with a download of
                the repository's release file. Network I/O can fail for any number of reasons but
                the most likely here is that there is a problem with the upstream repository which
                does have a status page we can direct the user to.
            ",
            CreatePackageIndexError::GetReleaseRequest(create_reqwest_middleware_error()),
            indoc! {"
                - Debug Info:
                  - Request error: error sending request for url (https://test/error)

                ! Failed to request Release file
                !
                ! While updating package sources, a request to download a Release file failed. This error \
                can occur due to an unstable network connection or an issue with the upstream Debian \
                package repository.
                !
                ! Suggestions:
                ! - Check the status of https://status.canonical.com/ for any reported issues.
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn create_package_index_error_read_get_release_response() {
        test_error_output(
            "
                Context
                -------
                Package sources are requested from a Debian repository starting with a download of
                the repository's release file. This error happens after the request has been initiated
                and we start reading the response body. Network I/O can fail for any number of reasons but
                the most likely here is that there is a problem with the upstream repository which
                does have a status page we can direct the user to.
            ",
            CreatePackageIndexError::ReadGetReleaseResponse(create_reqwest_error()),
            indoc! {"
                - Debug Info:
                  - error sending request for url (https://test/error)

                ! Failed to download Release file
                !
                ! While updating package sources, an error occurred while downloading a Release \
                file. This error can occur due to an unstable network connection or an issue with the upstream \
                Debian package repository.
                !
                ! Suggestions:
                ! - Check the status of https://status.canonical.com/ for any reported issues.
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn create_package_index_error_create_pgp_certificate() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. We validate
                release files using PGP certificates from the supported distributions and if there
                is a problem with the certificate file, this error would happen. Our testing processes
                should always catch this but, if not, we should direct users to file an issue.
            ",
            CreatePackageIndexError::CreatePgpCertificate(anyhow!(
                "Additional packets found, is this a keyring?"
            )),
            indoc! {"
                - Debug Info:
                  - Additional packets found, is this a keyring?

                ! Failed to load verifying PGP certificate
                !
                ! The PGP certificate used to verify downloaded release files failed to load. This error \
                indicates there's a problem with the format of the certificate file the \
                distribution uses.
                !
                ! Suggestions:
                ! - Verify the format of the certificates found in the ./keys directory of this \
                buildpack's repository. See https://cirw.in/gpg-decoder
                ! - Extract new certificates by running the ./scripts/extract_keys.sh script found \
                in this buildpack's repository.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_create_pgp_verifier() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. We validate
                release files using PGP certificates from the supported distributions and if there
                is a problem with how the release files are signed, this error would happen. Our
                testing processes should always catch this but, if not, we should direct users to file
                an issue.
            ",
            CreatePackageIndexError::CreatePgpVerifier(anyhow!("Malformed OpenPGP message")),
            indoc! {"
                - Debug Info:
                  - Malformed OpenPGP message

                ! Failed to verify Release file
                !
                ! The PGP signature of the downloaded release file failed verification. This error can \
                occur if the maintainers of the Debian repository changed the process \
                for signing release files.
                !
                ! Suggestions:
                ! - Verify if the keys changed by running the ./scripts/extract_keys.sh script \
                found in this buildpack's repository.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_write_release_layer() {
        test_error_output(
            "
                Context
                -------
                We write downloaded release files and other information into the release layer. I/O
                operations can fail for any number of reasons which we can't anticipate and since this
                file-system area is managed by the buildpack process there is nothing the user can do
                here other than report it.
            ",
            CreatePackageIndexError::WriteReleaseLayer(
                "/path/to/layer/file".into(),
                create_io_error("out of memory"),
            ),
            indoc! {"
                - Debug Info:
                  - out of memory

                ! Failed to write Release file to layer
                !
                ! An unexpected I/O error occurred while writing release data to `/path/to/layer/file`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_read_release_file() {
        test_error_output(
            "
                Context
                -------
                We read cached release files and other information from the release layer. I/O
                operations can fail for any number of reasons which we can't anticipate and since this
                file-system area is managed by the buildpack process there is nothing the user can do
                here other than report it.
            ",
            CreatePackageIndexError::ReadReleaseFile(
                "/path/to/layer/release-file".into(),
                create_io_error("not found"),
            ),
            indoc! {"
                - Debug Info:
                  - not found

                ! Failed to read Release file from layer
                !
                ! An unexpected I/O error occurred while reading Release data from `/path/to/layer/release-file`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_parse_release_file() {
        test_error_output(
            "
                Context
                -------
                If the release file cannot be parsed, this is either a developer error or a serious
                problem with the downloaded release file. Running the build with a clean cache
                should force the file to be re-downloaded which may correct the issue.
            ",
            CreatePackageIndexError::ParseReleaseFile(
                "/path/to/layer/release-file".into(),
                apt_parser::errors::ParseError.into(),
            ),
            indoc! {"
                - Debug Info:
                  - Failed to parse an APT value

                ! Failed to parse Release file data
                !
                ! We couldn't parse the Release file data stored in `/path/to/layer/release-file`. \
                This error is most likely a buildpack bug. It can also be caused by cached data \
                that's no longer valid or an issue with the upstream repository.
                !
                ! Suggestions:
                ! - Run the build again with a clean cache.
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn create_package_index_error_missing_sha256_release_hashes() {
        test_error_output(
            "
                Context
                -------
                If the release file downloaded from the Debian repository is missing the SHA256 release
                key then that's a serious problem with the repository.
            ",
            CreatePackageIndexError::MissingSha256ReleaseHashes(RepositoryUri::from(
                "http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease",
            )),
            indoc! {"
                ! Missing SHA256 Release hash
                !
                ! The Release file from http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease \
                is missing the SHA256 key which is required according to the documented Debian repository format. \
                This error is most likely an issue with the upstream repository. See \
                https://wiki.debian.org/DebianRepository/Format
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn create_package_index_error_missing_package_index_release_hash() {
        test_error_output(
            "
                Context
                -------
                If the release file downloaded from the Debian repository is missing the package index
                entry for a component then there's a serious problem with the repository.
            ",
            CreatePackageIndexError::MissingPackageIndexReleaseHash(
                RepositoryUri::from("http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease"),
                "main/binary-amd64/Packages.gz".to_string(),
            ),
            indoc! {"
                ! Missing Package Index
                !
                ! The Release file from http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease is \
                missing an entry for `main/binary-amd64/Packages.gz` within the SHA256 section. This error \
                is most likely a buildpack bug but can also be an issue with the upstream \
                repository.
                !
                ! Suggestions:
                ! - Verify if `main/binary-amd64/Packages.gz` is under the SHA256 section of \
                http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn create_package_index_error_get_packages_request() {
        test_error_output(
            "
                Context
                -------
                After downloading the release file, the next step for updating sources is to fetch
                the package indexes. Network I/O can fail for any number of reasons but
                the most likely here is that there is a problem with the upstream repository which
                does have a status page we can direct the user to.
            ",
            CreatePackageIndexError::GetPackagesRequest(create_reqwest_middleware_error()),
            indoc! {"
                - Debug Info:
                  - Request error: error sending request for url (https://test/error)

                ! Failed to request Package Index file
                !
                ! While updating package sources, a request to download a Package Index file failed. \
                This error can occur due to an unstable network connection or an issue with the upstream Debian \
                package repository.
                !
                ! Suggestions:
                ! - Check the status of https://status.canonical.com/ for any reported issues.
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn create_package_index_error_write_package_layer() {
        test_error_output(
            "
                Context
                -------
                When downloading a package index the response body stream is written directly to the
                file-system. This error could happen if the file to write to could not be opened.
            ",
            CreatePackageIndexError::WritePackagesLayer(
                "/path/to/layer/package".into(),
                create_io_error("entity already exists"),
            ),
            indoc! {"
                - Debug Info:
                  - entity already exists

                ! Failed to write Package Index file to layer
                !
                ! An unexpected I/O error occurred while writing Package Index data to `/path/to/layer/package`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_write_package_index_from_response() {
        test_error_output(
            "
                Context
                -------
                When downloading a package index the response body stream is written directly to the
                file-system. File and network I/O can fail for any number of reasons but the most
                likely here would be a connection interruption.
            ",
            CreatePackageIndexError::WritePackageIndexFromResponse(
                "/path/to/layer/package-index".into(),
                create_io_error("stream closed"),
            ),
            indoc! {"
                - Debug Info:
                  - stream closed

                ! Failed to download Package Index file
                !
                ! While updating package sources, an error occurred while downloading a Package Index \
                file to `/path/to/layer/package-index`. This error can occur due to an unstable network connection \
                or an issue with the upstream Debian package repository.
                !
                ! Suggestions:
                ! - Check the status of https://status.canonical.com/ for any reported issues.
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn create_package_index_error_checksum_failed() {
        test_error_output(
            "
                Context
                -------
                All downloaded package indexes are verified according to the checksum given by the
                owning release file. If these checksums don't match then the download is invalid. When
                this happens a rebuild typically fixes the issue.
            ",
            CreatePackageIndexError::ChecksumFailed {
                url: "http://ports.ubuntu.com/ubuntu-ports/dists/noble/main/binary-arm64/by-hash/SHA256/d41d8cd98f00b204e9800998ecf8427e".to_string(),
                expected: "d41d8cd98f00b204e9800998ecf8427e".to_string(),
                actual: "e62ff0123a74adfc6903d59a449cbdb0".to_string()
            },
            indoc! {"
                ! Package Index checksum verification failed
                !
                ! While updating package sources, an error occurred while verifying the checksum of \
                the Package Index at http://ports.ubuntu.com/ubuntu-ports/dists/noble/main/binary-arm64/by-hash/SHA256/d41d8cd98f00b204e9800998ecf8427e. \
                This error can occur due to an issue with the upstream Debian package repository.
                !
                ! Checksum:
                ! - Expected: `d41d8cd98f00b204e9800998ecf8427e`
                ! - Actual: `e62ff0123a74adfc6903d59a449cbdb0`
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn create_package_index_error_cpu_task_failed() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. Processing package
                index files is a CPU-intensive operation and happens in parallel worker tasks for
                efficiency. This error can happen if a worker task panics and the error isn't handled.
                Our testing processes should always catch this but, if not, we should direct users to file
                an issue.
            ",
            CreatePackageIndexError::CpuTaskFailed(create_recv_error()),
            indoc! {"
                - Debug Info:
                  - channel closed

                ! Task failure while reading Package Index data
                !
                ! A background task responsible for reading Package Index data failed to complete.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_read_packages_file() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. We read from
                a package index file stored in the layer directory managed by the buildpack process.
                I/O errors can happen for any number of reasons and there's nothing the user can
                do here.
            ",
            CreatePackageIndexError::ReadPackagesFile(
                "/path/to/layer/packages-file".into(),
                create_io_error("entity not found"),
            ),
            indoc! {"
                - Debug Info:
                  - entity not found

                ! Failed to read Package Index file
                !
                ! An unexpected I/O error occurred while reading Package Index data from \
                `/path/to/layer/packages-file`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn create_package_index_error_parse_packages() {
        test_error_output(
            "
                Context
                -------
                We need to parse all packages contained in a package index file. This error could happen
                if the package index contained bad data which would indicate a problem with the
                upstream repository but it's more likely to be a bug with the buildpack. Running the
                build with a fresh cache would force the package index to be re-downloaded which
                might fix the issue.
            ",
            CreatePackageIndexError::ParsePackages(
                "/path/to/layer/packages-file".into(),
                vec![
                    ParseRepositoryPackageError::MissingPackageName,
                    ParseRepositoryPackageError::MissingVersion("package-a".to_string()),
                    ParseRepositoryPackageError::MissingFilename("package-b".to_string()),
                    ParseRepositoryPackageError::MissingSha256("package-c".to_string()),
                ],
            ),
            indoc! {"
                ! Failed to parse Package Index file
                !
                ! We can't parse the Package Index file data stored in `/path/to/layer/packages-file`. \
                This error is most likely a buildpack bug. It can also be caused by \
                cached data that's no longer valid or an issue with the upstream repository.
                !
                ! Parsing errors:
                ! - There's an entry that's missing the required `Package` key.
                ! - Package `package-a` is missing the required `Version` key.
                ! - Package `package-b` is missing the required `Filename` key.
                ! - Package `package-c` is missing the required `SHA256` key.
                !
                ! Suggestions:
                ! - Run the build again with a clean cache.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn determine_packages_to_install_error_read_system_packages() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. All system packages
                are stored in /var/lib/dpkg/status. I/O errors can happen for any number of reasons
                but the most likely here is that the file doesn't exist for some reason or the file
                path was accidentally modified. Our testing processes should always catch this but,
                if not, we should direct users to file an issue.
            ",
            DeterminePackagesToInstallError::ReadSystemPackages(
                "/var/lib/dpkg/status".into(),
                create_io_error("entity not found"),
            ),
            indoc! {"
                - Debug Info:
                  - entity not found

                ! Failed to read system packages
                !
                ! An unexpected I/O error occurred while reading system packages from `/var/lib/dpkg/status`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn determine_packages_to_install_error_parse_system_packages() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. All system packages
                are stored in /var/lib/dpkg/status and it's unlikely for this file to be malformed. More
                likely is there is a bug with the parsing logic. Our testing processes should always catch
                this but, if not, we should direct users to file an issue.
            ",
            DeterminePackagesToInstallError::ParseSystemPackage(
                "/var/lib/dpkg/status".into(),
                "some-package".to_string(),
                apt_parser::errors::APTError::KVError(apt_parser::errors::KVError),
            ),
            indoc! {"
                - Debug Info:
                  - Failed to parse APT key-value data

                    Package:
                    some-package

                ! Failed to parse system package
                !
                ! An unexpected parsing error occurred while reading system packages from `/var/lib/dpkg/status`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn determine_packages_to_install_error_package_not_found() {
        test_error_output(
            "
                Context
                -------
                We're installing a list of packages given by the user in the buildpack configuration.
                It's possible to provide a valid name of a package that doesn't actually exist in the
                Debian repositories used by the distribution. If this happens we direct the user to
                the package search site for Ubuntu to verify the package name.
            ",
            DeterminePackagesToInstallError::PackageNotFound("some-package".to_string(), vec!["some-package1".to_string(), "some-package2".to_string()]),
            indoc! {"
                ! Package not found
                !
                ! We can't find `some-package` in the Package Index. If \
                this package is listed in the packages to install for this buildpack then the name is most \
                likely misspelled. Otherwise, it can be an issue with the \
                upstream Debian package repository.
                !
                ! Did you mean?
                ! - `some-package1`
                ! - `some-package2`
                !
                ! Suggestions:
                ! - Verify the package name is correct and exists for the target distribution at \
                https://packages.ubuntu.com/
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn determine_packages_to_install_error_package_not_found_with_no_suggestions() {
        test_error_output(
            "
                Context
                -------
                We're installing a list of packages given by the user in the buildpack configuration.
                It's possible to provide a valid name of a package that doesn't actually exist in the
                Debian repositories used by the distribution. If this happens we direct the user to
                the package search site for Ubuntu to verify the package name.
            ",
            DeterminePackagesToInstallError::PackageNotFound("some-package".to_string(), vec![]),
            indoc! {"
                ! Package not found
                !
                ! We can't find `some-package` in the Package Index. If \
                this package is listed in the packages to install for this buildpack then the name is most \
                likely misspelled. Otherwise, it can be an issue with the \
                upstream Debian package repository.
                !
                ! Did you mean?
                ! - No similarly named packages found
                !
                ! Suggestions:
                ! - Verify the package name is correct and exists for the target distribution at \
                https://packages.ubuntu.com/
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn determine_packages_to_install_error_virtual_package_must_be_specified() {
        test_error_output(
            "
                Context
                -------
                We're installing a list of packages given by the user in the buildpack configuration.
                There is a special type of package name in a Debian repository that represents a
                'virtual package' which may be implemented by one or more actual packages. This error
                is shown when there is more than one provider because we can't automatically determine
                which one should be used without the user's input.
            ",
            DeterminePackagesToInstallError::VirtualPackageMustBeSpecified(
                "some-package".to_string(),
                HashSet::from(["package-b".to_string(), "package-a".to_string()]),
            ),
            indoc! {"
                ! Multiple providers were found for the package `some-package`
                !
                ! Sometimes there are several packages which offer more-or-less the same functionality. \
                In this case, Debian repositories define a virtual package and one or more actual packages \
                provide an implementation for this virtual package. When multiple providers are found for \
                a requested package, this buildpack can't automatically choose which one is the desired \
                implementation.
                !
                ! Providing packages:
                ! - package-a
                ! - package-b
                !
                ! Suggestions:
                ! - Replace the virtual package `some-package` with one of the above providers.
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn install_packages_error_task_failed() {
        test_error_output_with_custom_assertion(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. Package installations
                are performed using async tasks which can fail if the task panics or is cancelled but
                we don't cancel running tasks and we handle all errors.
            ",
            InstallPackagesError::TaskFailed(create_join_error()),
            |actual_text| {
                assert_contains_match!(
                    actual_text,
                    indoc! {"
                    - Debug Info:
                      - task \\d+ panicked with message \"uh oh!\"

                    ! Task failure while installing packages
                    !
                    ! A background task responsible for installing failed to complete.
                    !
                    ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                    a workaround at this time. You can help our understanding by sharing your buildpack log \
                    and a description of the issue at:
                    ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                    !
                    ! If you're able to reproduce the problem with an example application and the `pack` \
                    build tool \\(https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/\\), \
                    adding that information to the discussion will also help. Once we have more information \
                    around the causes of this error we may update this message.
                "}
                );
            },
        );
    }

    #[test]
    fn install_packages_error_invalid_filename() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should never be seen by an end-user. Packages
                specify a Filename field that can be used to form a filename to download the
                package to. This error can only happen if that Filename field contains a value
                of '..' which is unlikely since that would cause serious problems for the upstream
                repository.
            ",
            InstallPackagesError::InvalidFilename("some-package".to_string(), "..".to_string()),
            indoc! {"
                ! Could not determine file name for `some-package`
                !
                ! The package information for `some-package` contains a Filename field of `..` which \
                produces an invalid name to use as a download path.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn install_packages_error_request_package() {
        test_error_output(
            "
                Context
                -------
                Packages are downloaded from a Debian repository. Network I/O can fail for any number
                of reasons but the most likely here would be a problem with the upstream.
            ",
            InstallPackagesError::RequestPackage(
                repository_package("some-package"),
                create_reqwest_middleware_error(),
            ),
            indoc! {"
                - Debug Info:
                  - Request error: error sending request for url (https://test/error)

                ! Failed to request package
                !
                ! While installing packages, an error occurred while downloading `some-package`. This error \
                can occur due to an unstable network connection or an issue with the upstream Debian package \
                repository.
                !
                ! Suggestions:
                ! - Check the status of https://status.canonical.com/ for any reported issues.
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn install_packages_error_write_package() {
        test_error_output(
            "
                Context
                -------
                Packages downloaded from a Debian repository are written directly to disk. Network I/O
                can fail for any number of reasons but the most likely here would be a problem with the upstream.
            ",
            InstallPackagesError::WritePackage(
                repository_package("some-package"),
                "https://test/error".to_string(),
                "/path/to/layer/download-file".into(),
                create_io_error("stream closed"),
            ),
            indoc! {"
                - Debug Info:
                  - stream closed

                ! Failed to download package
                !
                ! An unexpected I/O error occured while downloading `some-package` from https://test/error \
                to `/path/to/layer/download-file`. This error can occur due to an unstable network connection or \
                an issue with the upstream Debian package repository.
                !
                ! Suggestions:
                ! - Check the status of https://status.canonical.com/ for any reported issues.
                !
                ! Use the debug information above to troubleshoot and retry your build.
                !
                ! If the issue persists and you think you found a bug in the buildpack, reproduce the \
                issue locally with a minimal example. Open an issue in the buildpack's GitHub repository \
                and include the details here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    #[test]
    fn install_packages_error_checksum_failed() {
        test_error_output(
            "
                Context
                -------
                Packages downloaded from a Debian repository are validated against a checksum given by
                their owning package index. If this error happens then there was a problem with the
                download. When this happens, running the build again typically fixes it.
            ",
            InstallPackagesError::ChecksumFailed {
                url: "http://archive.ubuntu.com/ubuntu/dists/jammy/some-package.tgz".to_string(),
                expected: "7931f51fd704f93171f36f5f6f1d7b7b".into(),
                actual: "19a47cdb280539511523382fa1cabbe5".to_string(),
            },
            indoc! {"
                ! Package checksum verification failed
                !
                ! An error occurred while verifying the checksum of the package at \
                http://archive.ubuntu.com/ubuntu/dists/jammy/some-package.tgz. This error can occur due to an \
                issue with the upstream Debian package repository.
                !
                ! Checksum:
                ! - Expected: `7931f51fd704f93171f36f5f6f1d7b7b`
                ! - Actual: `19a47cdb280539511523382fa1cabbe5`
                !
                ! Use the debug information above to troubleshoot and retry your build.
            "},
        );
    }

    #[test]
    fn install_packages_error_open_package_archive() {
        test_error_output(
            "
                Context
                -------
                Packages downloaded from a Debian repository are stored as tarballs which need to be
                opened for extraction. I/O can fail for any number of reasons but since the buildpack
                process owns this content, there's nothing the user can do here.
            ",
            InstallPackagesError::OpenPackageArchive(
                "/path/to/layer/archive-file.tgz".into(),
                create_io_error("permission denied"),
            ),
            indoc! {"
                - Debug Info:
                  - permission denied

                ! Failed to open package archive
                !
                ! An unexpected I/O error occurred while trying to open the archive at `/path/to/layer/archive-file.tgz`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn install_packages_error_open_package_archive_entry() {
        test_error_output(
            "
                Context
                -------
                Packages downloaded from a Debian repository are stored as tarballs which stores file
                information in individual file entries which need to be read for extraction. I/O
                can fail for any number of reasons but since the buildpack process owns this content,
                there's nothing the user can do here.
            ",
            InstallPackagesError::OpenPackageArchiveEntry(
                "/path/to/layer/archive-file.tgz".into(),
                create_io_error("invalid header entry"),
            ),
            indoc! {"
                - Debug Info:
                  - invalid header entry

                ! Failed to read package archive entry
                !
                ! An unexpected I/O error occurred while trying to read the entries of the archive at \
                `/path/to/layer/archive-file.tgz`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn install_packages_error_unpack_tarball() {
        test_error_output(
            "
                Context
                -------
                Packages downloaded from a Debian repository are stored as tarballs which need to be
                extracted. This is done by iterating through each archive entry and writing it's
                content out as a file to the file-system. I/O can fail for any number of reasons but
                since the buildpack process owns this content, there's nothing the user can do here.
            ",
            InstallPackagesError::UnpackTarball(
                "/path/to/layer/archive-file.tgz".into(),
                create_io_error("directory not empty"),
            ),
            indoc! {"
                - Debug Info:
                  - directory not empty

                ! Failed to unpack package archive
                !
                ! An unexpected I/O error occurred while trying to unpack the archive at `/path/to/layer/archive-file.tgz`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn install_packages_error_unsupported_compression() {
        test_error_output(
            "
                Context
                -------
                This is a developer error. It should not be seen by an end-user. Packages from a Debian
                repository can be stored using several different compression formats. Should we encounter
                an unexpected one that we aren't handling then this is a buildpack bug.
            ",
            InstallPackagesError::UnsupportedCompression(
                "/path/to/layer/archive-file.tgz".into(),
                "lz".to_string(),
            ),
            indoc! {"
                ! Unsupported compression format for package archive
                !
                ! An unexpected compression format (`lz`) was used for the package archive at \
                `/path/to/layer/archive-file.tgz`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn install_packages_error_read_package_config() {
        test_error_output(
            "
                Context
                -------
                After a package is extracted, we need to update any hardcoded references to it's standard
                install location that might be referenced in any package-config (*.pc) files from the
                package to reflect the install location within the layer directory by reading these files.
                I/O can fail for any number of reasons but since the buildpack process owns this content,
                there's nothing  the user can do here.
            ",
            InstallPackagesError::ReadPackageConfig(
                "/path/to/layer/pkgconfig/somepackage.pc".into(),
                create_io_error("invalid filename"),
            ),
            indoc! {"
                - Debug Info:
                  - invalid filename

                ! Failed to read package config file
                !
                ! An unexpected I/O error occurred while reading the package config file at \
                `/path/to/layer/pkgconfig/somepackage.pc`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn install_packages_error_write_package_config() {
        test_error_output(
            "
                Context
                -------
                After a package is extracted, we need to update any hardcoded references to it's standard
                install location that might be referenced in any package-config (*.pc) files from the
                package to reflect the install location within the layer directory by writing these files.
                I/O can fail for any number of reasons but since the buildpack process owns this content,
                there's nothing  the user can do here.
            ",
            InstallPackagesError::WritePackageConfig(
                "/path/to/layer/pkgconfig/somepackage.pc".into(),
                create_io_error("operation interrupted"),
            ),
            indoc! {"
                - Debug Info:
                  - operation interrupted

                ! Failed to write package config file
                !
                ! An unexpected I/O error occurred while writing the package config file to \
                `/path/to/layer/pkgconfig/somepackage.pc`.
                !
                ! The causes for this error are unknown. We do not have suggestions for diagnosis or \
                a workaround at this time. You can help our understanding by sharing your buildpack log \
                and a description of the issue at:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
                !
                ! If you're able to reproduce the problem with an example application and the `pack` \
                build tool (https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/), \
                adding that information to the discussion will also help. Once we have more information \
                around the causes of this error we may update this message.
            "},
        );
    }

    #[test]
    fn framework_error() {
        test_error_output(
            "
                Context
                -------
                This message is for any framework errors caused by libcnb.rs and should be consistent
                with how our other buildpacks report framework errors.
            ",
            Error::CannotWriteBuildSbom(create_io_error("operation interrupted")),
            indoc! {"
                - Debug Info:
                  - Couldn't write build SBOM files: operation interrupted

                ! Heroku Deb Packages Buildpack internal error
                !
                ! The framework used by this buildpack encountered an unexpected error.
                !
                ! If you can’t deploy to Heroku due to this issue, check the official Heroku Status \
                page at status.heroku.com for any ongoing incidents. After all incidents resolve, retry \
                your build.
                !
                ! Use the debug information above to troubleshoot and retry your build. If you think you \
                found a bug in the buildpack, reproduce the issue locally with a minimal example and file \
                an issue here:
                ! https://github.com/heroku/buildpacks-deb-packages/issues/new
            "},
        );
    }

    fn test_error_output(
        context: &str,
        error: impl Into<Error<DebianPackagesBuildpackError>>,
        expected_text: &str,
    ) {
        test_error_output_with_custom_assertion(context, error, |actual_text| {
            assert_eq!(normalize_text(&actual_text), normalize_text(expected_text));
        });
    }

    fn test_error_output_with_custom_assertion(
        // this is present to enforce adding contextual information for the error to be used in reviews
        _context: &str,
        error: impl Into<Error<DebianPackagesBuildpackError>>,
        assert_fn: impl FnOnce(String),
    ) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let reader = file.reopen().unwrap();
        let writer = strip_ansi_escapes::Writer::new(file);
        on_error(error.into(), writer);
        let actual_text = std::io::read_to_string(reader).unwrap();
        assert_fn(actual_text);
    }

    fn normalize_text(input: impl AsRef<str>) -> String {
        // this transformation helps to reduce some whitespace noise seen when doing output comparisons
        input
            .as_ref()
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn create_io_error(text: &str) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, text)
    }

    fn async_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    fn create_join_error() -> tokio::task::JoinError {
        async_runtime().block_on(async {
            tokio::spawn(async {
                panic!("uh oh!");
            })
            .await
            .unwrap_err()
        })
    }

    fn create_recv_error() -> tokio::sync::oneshot::error::RecvError {
        async_runtime().block_on(async {
            let (send, recv) = tokio::sync::oneshot::channel::<u32>();
            tokio::spawn(async move {
                drop(send);
            });
            recv.await.unwrap_err()
        })
    }

    fn create_reqwest_middleware_error() -> reqwest_middleware::Error {
        create_reqwest_error().into()
    }

    fn create_reqwest_error() -> reqwest::Error {
        async_runtime().block_on(async { reqwest::get("https://test/error").await.unwrap_err() })
    }

    fn repository_package(package_name: &str) -> RepositoryPackage {
        RepositoryPackage {
            name: package_name.to_string(),
            version: "1.0.0".to_string(),
            filename: format!("{package_name}.tgz"),
            repository_uri: RepositoryUri::from("https://test/path/to/repository"),
            sha256sum: String::new(),
            depends: None,
            pre_depends: None,
            provides: None,
        }
    }
}
