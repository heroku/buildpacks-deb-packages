use crate::config::custom_source::ParseCustomSourceError;
use crate::config::download_url::ParseDownloadUrlError;
use crate::config::{ConfigError, NAMESPACED_CONFIG, ParseConfigError, ParseRequestedPackageError};
use crate::create_package_index::CreatePackageIndexError;
use crate::debian::UnsupportedDistroError;
use crate::determine_packages_to_install::DeterminePackagesToInstallError;
use crate::errors::ErrorType::{Framework, Internal, UserFacing};
use crate::install_packages::InstallPackagesError;
use crate::{DebianPackagesBuildpackError, DetectError};
use bon::builder;
use bullet_stream::{Print, global::print, style};
use indoc::{formatdoc, indoc};
use libcnb::Error;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

const BUILDPACK_NAME: &str = "Heroku .deb Packages buildpack";

pub(crate) fn on_error(error: Error<DebianPackagesBuildpackError>) {
    print_error(match error {
        Error::BuildpackError(e) => on_buildpack_error(e),
        e => on_framework_error(&e),
    });
}

fn on_buildpack_error(error: DebianPackagesBuildpackError) -> ErrorMessage {
    match error {
        DebianPackagesBuildpackError::Config(e) => on_config_error(e),
        DebianPackagesBuildpackError::UnsupportedDistro(e) => on_unsupported_distro_error(e),
        DebianPackagesBuildpackError::CreatePackageIndex(e) => on_create_package_index_error(e),
        DebianPackagesBuildpackError::DeterminePackagesToInstall(e) => {
            on_determine_packages_to_install_error(*e)
        }
        DebianPackagesBuildpackError::InstallPackages(e) => on_install_packages_error(*e),
        DebianPackagesBuildpackError::Detect(e) => on_detect_error(e),
    }
}

#[allow(clippy::too_many_lines)]
fn on_config_error(error: ConfigError) -> ErrorMessage {
    match error {
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
            let root_config_key = style::value(format!("[{NAMESPACED_CONFIG}]"));
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

                ParseConfigError::ParseRequestedPackage(error) => match *error {
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

                ParseConfigError::MissingNamespacedConfig => {
                    create_error()
                        .error_type(UserFacing(SuggestRetryBuild::No, SuggestSubmitIssue::No))
                        .header(format!("Error parsing {config_file} with invalid key"))
                        .body(formatdoc! { "
                            The {BUILDPACK_NAME} reads the configuration from {config_file} to complete \
                            the build but no configuration for the key {root_config_key} is present. The \
                            value of this key must be a TOML table.

                            Suggestions:
                            - See the buildpack documentation for the proper usage for this configuration at \
                            {configuration_doc_url}
                            - See the TOML documentation for more details on the TOML table type at \
                            {toml_spec_url}
                        " })
                        .call()
                }

                ParseConfigError::ParseCustomSource(error) => {
                    let custom_source_array_of_tables_key = "[[com.heroku.buildpacks.deb-packages.sources]]";
                    create_error()
                        .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                        .header(format!("Error parsing {config_file} with invalid custom source"))
                        .body(formatdoc! { r#"
                            The {BUILDPACK_NAME} reads configuration from {config_file} to \
                            complete the build but we found an invalid custom source in the \
                            key {root_config_key}.

                            Custom sources must be in the following format:

                            {custom_source_array_of_tables_key}
                            uri = "<url_of_debian_repository> (e.g.; http://archive.ubuntu.com/ubuntu)"
                            suites = ["<suite> (e.g.; jammy)"]
                            components = ["<component> (e.g.; main)"]
                            arch = ["<architecture> (e.g.; amd64 or arm64)"]
                            signed_by = """-----BEGIN PGP PUBLIC KEY BLOCK-----
                            <ASCII-armored GPG key>
                            -----END PGP PUBLIC KEY BLOCK-----

                            Suggestions:
                            - See the buildpack documentation for the proper usage for this configuration at \
                            {configuration_doc_url}
                            - See the TOML documentation for more details on the TOML array of tables type \
                            at {toml_spec_url}
                            "# })
                        .debug_info(match *error {
                            ParseCustomSourceError::MissingUri(table) => formatdoc! { "
                                Missing or invalid \"uri\" field for the following custom source:
                                {custom_source_array_of_tables_key}
                                {table}
                            " },
                            ParseCustomSourceError::MissingSignedBy(table) => formatdoc! { "
                                Missing or invalid \"signed_by\" field for the following custom source:
                                {custom_source_array_of_tables_key}
                                {table}
                            " },
                            ParseCustomSourceError::MissingSuites(table) => formatdoc! { "
                                Missing or invalid \"suites\" field. One or more String values must be present for the following custom source:
                                {custom_source_array_of_tables_key}
                                {table}
                            " },
                            ParseCustomSourceError::MissingComponents(table) => formatdoc! { "
                                Missing or invalid \"components\" field. One or more String values must be present for the following custom source:
                                {custom_source_array_of_tables_key}
                                {table}
                            " },
                            ParseCustomSourceError::MissingArchitectureNames(table) => formatdoc! { "
                                Missing or invalid \"arch\" field. One or more String values must be present for the following custom source:
                                {custom_source_array_of_tables_key}
                                {table}
                            " },
                            ParseCustomSourceError::UnexpectedTomlValue(table, value) => formatdoc! { "
                                Unexpected toml value (\"{value}\") found for the following custom source:
                                {custom_source_array_of_tables_key}
                                {table}
                            " },
                            ParseCustomSourceError::InvalidArchitectureName(table, e) => formatdoc! { "
                                Invalid architecture name found for the following custom source:
                                {custom_source_array_of_tables_key}
                                {table}
                                ---
                                {e}
                            " },
                        })
                        .call()
                }

                ParseConfigError::ParseDownloadUrl(error) => match *error {
                    ParseDownloadUrlError::InvalidUrl { url, reason } => {
                        let url = style::value(url);
                        create_error()
                            .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                            .header(format!("Error parsing {config_file} with invalid download url"))
                            .body(formatdoc! { "
                                The {BUILDPACK_NAME} reads configuration from {config_file} to \
                                complete the build but we found an invalid download url {url} \
                                in the key {root_config_key}.

                                Validation error: {reason}

                                Suggestions:
                                - Verify the download url is valid.
                            " })
                            .call()
                    }
                    ParseDownloadUrlError::UnexpectedTomlValue(value) => {
                        let string_example = "\"https://example.com/package-1.2.3.deb\"";
                        let value_type = style::value(value.type_name());
                        let value = style::value(value.to_string());

                        create_error()
                            .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                            .header(format!("Error parsing {config_file} with invalid download url"))
                            .body(formatdoc! { "
                                The {BUILDPACK_NAME} reads configuration from {config_file} to \
                                complete the build but we found an invalid download url in the \
                                key {root_config_key}.

                                Download urls must either be the following TOML values:
                                - String (e.g.; {string_example})

                                Suggestions:
                                - See the buildpack documentation for the proper usage for this configuration at \
                                {configuration_doc_url}
                                - See the TOML documentation for more details on the TOML string at {toml_spec_url}
                            " })
                            .debug_info(format!("Invalid type {value_type} with value {value}"))
                            .call()
                    }
                }
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

    let buildpack_toml = style::value("buildpack.toml");
    create_error()
        .error_type(Internal)
        .header("Unsupported distribution")
        .body(formatdoc! { "
            The {BUILDPACK_NAME} doesn't support the {name} {version} ({architecture}) \
            distribution. See {buildpack_toml} for the configuration of supported distributions.
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
            let release_uri = style::url(&release_uri);
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
            let release_uri = style::url(&release_uri);
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

        InstallPackagesError::RequestPackageUrl(download_url, e) => {
            let url = style::url(download_url.to_string());
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to request package from download url")
                .body(formatdoc! { "
                    While installing packages, an error occurred while downloading the package at {url}. \
                    This error can occur due to an unstable network connection or an issue \
                    with site this package is hosted at.

                    Suggestions:
                    - Check if {url} can be downloaded locally or if there's an error.
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

        InstallPackagesError::WritePackageUrl(download_url, destination_path, e) => {
            let download_url = style::url(download_url.to_string());
            let destination_path = file_value(destination_path);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::Yes))
                .header("Failed to download package")
                .body(formatdoc! { "
                    An unexpected I/O error occured while downloading the package at {download_url} \
                    to {destination_path}. This error can occur due to an unstable network connection or an issue \
                    with the site this packages is hosted at.

                    Suggestions:
                    - Check if {download_url} can be downloaded locally or if there's an error.
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

fn on_detect_error(error: DetectError) -> ErrorMessage {
    match error {
        DetectError::CheckExistsAptfile(file, e) | DetectError::CheckExistsProjectToml(file, e) => {
            let file = file_value(file);
            create_error()
                .error_type(UserFacing(SuggestRetryBuild::Yes, SuggestSubmitIssue::No))
                .header("Unable to complete buildpack detection")
                .body(formatdoc! { "
                    An unexpected I/O error occurred while checking {file} to determine if the \
                    {BUILDPACK_NAME} is compatible for this application.
                " })
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

fn print_error(error_message: ErrorMessage) {
    if let Some(debug_info) = error_message.debug_info {
        print::bullet(style::important("Debug Info:"));
        print::sub_bullet(debug_info);
    }
    print::error(error_message.message);
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

impl fmt::Display for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut log = Print::new(vec![]).without_header();
        if let Some(debug_info) = &self.debug_info {
            log = log
                .bullet(style::important("Debug Info:"))
                .sub_bullet(debug_info)
                .done();
        }
        let output = log.error(&self.message);
        write!(f, "{}", String::from_utf8_lossy(&output))
    }
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
    use crate::config::download_url::DownloadUrl;
    use crate::debian::{
        ParsePackageNameError, ParseRepositoryPackageError, RepositoryPackage, RepositoryUri,
        UnsupportedArchitectureNameError,
    };
    use anyhow::anyhow;
    use bullet_stream::strip_ansi;
    use insta::{assert_snapshot, with_settings};
    use libcnb::data::layer::LayerNameError;
    use reqwest::Url;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::str::FromStr;
    use toml_edit::Table;

    #[test]
    fn test_detect_check_exists_project_toml_error() {
        assert_error_snapshot(&on_detect_error(DetectError::CheckExistsProjectToml(
            "/path/to/project.toml".into(),
            create_io_error("test I/O error"),
        )));
    }

    #[test]
    fn test_detect_check_exists_aptfile_error() {
        assert_error_snapshot(&on_detect_error(DetectError::CheckExistsAptfile(
            "/path/to/Aptfile".into(),
            create_io_error("test I/O error"),
        )));
    }

    #[test]
    fn config_read_config_error() {
        assert_error_snapshot(&on_config_error(ConfigError::ReadConfig(
            "/path/to/project.toml".into(),
            create_io_error("test I/O error"),
        )));
    }

    #[test]
    fn config_parse_config_error_for_wrong_config_type() {
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::WrongConfigType,
        )));
    }

    #[test]
    fn config_parse_config_error_for_invalid_toml() {
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::InvalidToml(
                toml_edit::DocumentMut::from_str("[com.heroku").unwrap_err(),
            ),
        )));
    }

    #[test]
    fn config_parse_config_error_for_invalid_package_name() {
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseRequestedPackage(Box::from(
                ParseRequestedPackageError::InvalidPackageName(ParsePackageNameError {
                    package_name: "invalid!package!name".to_string(),
                }),
            )),
        )));
    }

    #[test]
    fn config_parse_config_error_for_invalid_package_name_config_type() {
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseRequestedPackage(Box::from(
                ParseRequestedPackageError::UnexpectedTomlValue(
                    toml_edit::value(37).into_value().unwrap(),
                ),
            )),
        )));
    }

    #[test]
    fn config_parse_config_error_for_missing_namespaced_config() {
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::MissingNamespacedConfig,
        )));
    }

    #[test]
    fn config_parse_config_error_for_custom_source_with_missing_uri() {
        let mut table = create_custom_source_table();
        table.remove("uri");
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseCustomSource(Box::from(ParseCustomSourceError::MissingUri(
                table,
            ))),
        )));
    }

    #[test]
    fn config_parse_config_error_for_custom_source_with_missing_signed_by() {
        let mut table = create_custom_source_table();
        table.remove("signed_by");
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseCustomSource(Box::from(
                ParseCustomSourceError::MissingSignedBy(table),
            )),
        )));
    }

    #[test]
    fn config_parse_config_error_for_custom_source_with_missing_suites() {
        let mut table = create_custom_source_table();
        table.remove("suites");
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseCustomSource(Box::from(ParseCustomSourceError::MissingSuites(
                table,
            ))),
        )));
    }

    #[test]
    fn config_parse_config_error_for_custom_source_with_missing_components() {
        let mut table = create_custom_source_table();
        table.remove("components");
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseCustomSource(Box::from(
                ParseCustomSourceError::MissingComponents(table),
            )),
        )));
    }

    #[test]
    fn config_parse_config_error_for_custom_source_with_invalid_components() {
        let mut table = create_custom_source_table();
        let mut components = toml_edit::Array::new();
        components.push(123);
        table.insert(
            "components",
            toml_edit::Item::Value(toml_edit::Value::from(components)),
        );
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseCustomSource(Box::from(
                ParseCustomSourceError::UnexpectedTomlValue(
                    table,
                    toml_edit::value(123).into_value().unwrap(),
                ),
            )),
        )));
    }

    #[test]
    fn config_parse_config_error_for_custom_source_with_missing_architecture_names() {
        let mut table = create_custom_source_table();
        table.remove("arch");
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseCustomSource(Box::from(
                ParseCustomSourceError::MissingArchitectureNames(table),
            )),
        )));
    }

    #[test]
    fn config_parse_config_error_for_custom_source_with_invalid_architecture_name() {
        let mut table = create_custom_source_table();
        let mut arch = toml_edit::Array::new();
        arch.push("i386");
        table.insert("arch", toml_edit::Item::Value(toml_edit::Value::from(arch)));
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseCustomSource(Box::from(
                ParseCustomSourceError::InvalidArchitectureName(
                    table,
                    UnsupportedArchitectureNameError("i386".into()),
                ),
            )),
        )));
    }

    #[test]
    fn config_parse_config_error_for_invalid_download_url() {
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseDownloadUrl(Box::from(ParseDownloadUrlError::InvalidUrl {
                url: "not a url".into(),
                reason: Url::parse("not a url").unwrap_err().to_string(),
            })),
        )));
    }

    #[test]
    fn config_parse_config_error_for_invalid_download_url_config_type() {
        assert_error_snapshot(&on_config_error(ConfigError::ParseConfig(
            "/path/to/project.toml".into(),
            ParseConfigError::ParseDownloadUrl(Box::from(
                ParseDownloadUrlError::UnexpectedTomlValue(
                    toml_edit::value(37).into_value().unwrap(),
                ),
            )),
        )));
    }

    #[test]
    fn unsupported_distro_error() {
        assert_error_snapshot(&on_unsupported_distro_error(UnsupportedDistroError {
            name: "Windows".to_string(),
            version: "XP".to_string(),
            architecture: "x86".to_string(),
        }));
    }

    #[test]
    fn create_package_index_error_no_sources() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::NoSources,
        ));
    }

    #[test]
    fn create_package_index_error_task_failed() {
        assert_error_snapshot_with_filters(
            &on_create_package_index_error(
                CreatePackageIndexError::TaskFailed(create_join_error()),
            ),
            vec![("task \\d+", "task <ID>")],
        );
    }

    #[test]
    fn create_package_index_error_invalid_layer_name() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::InvalidLayerName(
                "http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease".to_string(),
                LayerNameError::InvalidValue(
                    "623e1a2085e65abc3dc626a97909466ce19efe37f7a6529a842c290fcfc65b3b".to_string(),
                ),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_get_release_request() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::GetReleaseRequest(create_reqwest_middleware_error()),
        ));
    }

    #[test]
    fn create_package_index_error_read_get_release_response() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::ReadGetReleaseResponse(create_reqwest_error()),
        ));
    }

    #[test]
    fn create_package_index_error_create_pgp_certificate() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::CreatePgpCertificate(anyhow!(
                "Additional packets found, is this a keyring?"
            )),
        ));
    }

    #[test]
    fn create_package_index_error_create_pgp_verifier() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::CreatePgpVerifier(anyhow!("Malformed OpenPGP message")),
        ));
    }

    #[test]
    fn create_package_index_error_write_release_layer() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::WriteReleaseLayer(
                "/path/to/layer/file".into(),
                create_io_error("out of memory"),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_read_release_file() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::ReadReleaseFile(
                "/path/to/layer/release-file".into(),
                create_io_error("not found"),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_parse_release_file() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::ParseReleaseFile(
                "/path/to/layer/release-file".into(),
                apt_parser::errors::ParseError.into(),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_missing_sha256_release_hashes() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::MissingSha256ReleaseHashes(RepositoryUri::from(
                "http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease",
            )),
        ));
    }

    #[test]
    fn create_package_index_error_missing_package_index_release_hash() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::MissingPackageIndexReleaseHash(
                RepositoryUri::from("http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease"),
                "main/binary-amd64/Packages.gz".to_string(),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_get_packages_request() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::GetPackagesRequest(create_reqwest_middleware_error()),
        ));
    }

    #[test]
    fn create_package_index_error_write_package_layer() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::WritePackagesLayer(
                "/path/to/layer/package".into(),
                create_io_error("entity already exists"),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_write_package_index_from_response() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::WritePackageIndexFromResponse(
                "/path/to/layer/package-index".into(),
                create_io_error("stream closed"),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_checksum_failed() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::ChecksumFailed {
                url: "http://ports.ubuntu.com/ubuntu-ports/dists/noble/main/binary-arm64/by-hash/SHA256/d41d8cd98f00b204e9800998ecf8427e".to_string(),
                expected: "d41d8cd98f00b204e9800998ecf8427e".to_string(),
                actual: "e62ff0123a74adfc6903d59a449cbdb0".to_string(),
            },
        ));
    }

    #[test]
    fn create_package_index_error_cpu_task_failed() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::CpuTaskFailed(create_recv_error()),
        ));
    }

    #[test]
    fn create_package_index_error_read_packages_file() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::ReadPackagesFile(
                "/path/to/layer/packages-file".into(),
                create_io_error("entity not found"),
            ),
        ));
    }

    #[test]
    fn create_package_index_error_parse_packages() {
        assert_error_snapshot(&on_create_package_index_error(
            CreatePackageIndexError::ParsePackages(
                "/path/to/layer/packages-file".into(),
                vec![
                    ParseRepositoryPackageError::MissingPackageName,
                    ParseRepositoryPackageError::MissingVersion("package-a".to_string()),
                    ParseRepositoryPackageError::MissingFilename("package-b".to_string()),
                    ParseRepositoryPackageError::MissingSha256("package-c".to_string()),
                ],
            ),
        ));
    }

    #[test]
    fn determine_packages_to_install_error_read_system_packages() {
        assert_error_snapshot(&on_determine_packages_to_install_error(
            DeterminePackagesToInstallError::ReadSystemPackages(
                "/var/lib/dpkg/status".into(),
                create_io_error("entity not found"),
            ),
        ));
    }

    #[test]
    fn determine_packages_to_install_error_parse_system_packages() {
        assert_error_snapshot(&on_determine_packages_to_install_error(
            DeterminePackagesToInstallError::ParseSystemPackage(
                "/var/lib/dpkg/status".into(),
                "some-package".to_string(),
                apt_parser::errors::APTError::KVError(apt_parser::errors::KVError),
            ),
        ));
    }

    #[test]
    fn determine_packages_to_install_error_package_not_found() {
        assert_error_snapshot(&on_determine_packages_to_install_error(
            DeterminePackagesToInstallError::PackageNotFound(
                "some-package".to_string(),
                vec!["some-package1".to_string(), "some-package2".to_string()],
            ),
        ));
    }

    #[test]
    fn determine_packages_to_install_error_package_not_found_with_no_suggestions() {
        assert_error_snapshot(&on_determine_packages_to_install_error(
            DeterminePackagesToInstallError::PackageNotFound("some-package".to_string(), vec![]),
        ));
    }

    #[test]
    fn determine_packages_to_install_error_virtual_package_must_be_specified() {
        assert_error_snapshot(&on_determine_packages_to_install_error(
            DeterminePackagesToInstallError::VirtualPackageMustBeSpecified(
                "some-package".to_string(),
                HashSet::from(["package-b".to_string(), "package-a".to_string()]),
            ),
        ));
    }

    #[test]
    fn install_packages_error_task_failed() {
        assert_error_snapshot_with_filters(
            &on_install_packages_error(InstallPackagesError::TaskFailed(create_join_error())),
            vec![("task \\d+", "task <ID>")],
        );
    }

    #[test]
    fn install_packages_error_invalid_filename() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::InvalidFilename("some-package".to_string(), "..".to_string()),
        ));
    }

    #[test]
    fn install_packages_error_request_package() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::RequestPackage(
                repository_package("some-package"),
                create_reqwest_middleware_error(),
            ),
        ));
    }

    #[test]
    fn install_packages_error_request_package_url() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::RequestPackageUrl(
                DownloadUrl::from_str("https://example.com/custom-package.deb").unwrap(),
                create_reqwest_middleware_error(),
            ),
        ));
    }

    #[test]
    fn install_packages_error_write_package() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::WritePackage(
                repository_package("some-package"),
                "https://test/error".to_string(),
                "/path/to/layer/download-file".into(),
                create_io_error("stream closed"),
            ),
        ));
    }

    #[test]
    fn install_packages_error_write_package_url() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::WritePackageUrl(
                DownloadUrl::from_str("https://example.com/custom-package.deb").unwrap(),
                "/path/to/layer/custom-package.deb".into(),
                create_io_error("connection reset"),
            ),
        ));
    }

    #[test]
    fn install_packages_error_checksum_failed() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::ChecksumFailed {
                url: "http://archive.ubuntu.com/ubuntu/dists/jammy/some-package.tgz".to_string(),
                expected: "7931f51fd704f93171f36f5f6f1d7b7b".into(),
                actual: "19a47cdb280539511523382fa1cabbe5".to_string(),
            },
        ));
    }

    #[test]
    fn install_packages_error_open_package_archive() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::OpenPackageArchive(
                "/path/to/layer/archive-file.tgz".into(),
                create_io_error("permission denied"),
            ),
        ));
    }

    #[test]
    fn install_packages_error_open_package_archive_entry() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::OpenPackageArchiveEntry(
                "/path/to/layer/archive-file.tgz".into(),
                create_io_error("invalid header entry"),
            ),
        ));
    }

    #[test]
    fn install_packages_error_unpack_tarball() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::UnpackTarball(
                "/path/to/layer/archive-file.tgz".into(),
                create_io_error("directory not empty"),
            ),
        ));
    }

    #[test]
    fn install_packages_error_unsupported_compression() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::UnsupportedCompression(
                "/path/to/layer/archive-file.tgz".into(),
                "lz".to_string(),
            ),
        ));
    }

    #[test]
    fn install_packages_error_read_package_config() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::ReadPackageConfig(
                "/path/to/layer/pkgconfig/somepackage.pc".into(),
                create_io_error("invalid filename"),
            ),
        ));
    }

    #[test]
    fn install_packages_error_write_package_config() {
        assert_error_snapshot(&on_install_packages_error(
            InstallPackagesError::WritePackageConfig(
                "/path/to/layer/pkgconfig/somepackage.pc".into(),
                create_io_error("operation interrupted"),
            ),
        ));
    }

    #[test]
    fn framework_error() {
        let error = Error::CannotWriteBuildSbom(create_io_error("operation interrupted"));
        assert_error_snapshot(&on_framework_error(&error));
    }

    fn assert_error_snapshot(error: &ErrorMessage) {
        assert_error_snapshot_with_filters(error, vec![]);
    }

    fn assert_error_snapshot_with_filters(error: &ErrorMessage, filters: Vec<(&str, &str)>) {
        let output = strip_ansi(error.to_string());
        let test_name = std::thread::current()
            .name()
            .expect("Test name should be available as the current thread name")
            .replace("::", "_")
            .replace("_tests", "");
        let snapshot_path = std::env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .expect(
                "The CARGO_MANIFEST_DIR should be automatically set by Cargo when running tests",
            )
            .join("src/__snapshots");
        with_settings!({
            prepend_module_to_snapshot => false,
            omit_expression => true,
            snapshot_path => snapshot_path,
            filters => filters,
        }, {
            assert_snapshot!(test_name, output);
        });
    }

    fn create_custom_source_table() -> Table {
        toml_edit::DocumentMut::from_str(indoc! { r#"
            uri = "http://archive.ubuntu.com/ubuntu"
            suites = ["main"]
            components = ["multiverse"]
            arch = ["amd64", "arm64"]
            signed_by = """-----BEGIN PGP PUBLIC KEY BLOCK-----

            NxRt3Z+7w5HMIN2laKp+ItxloPWGBdcHU4o2ZnWgsVT8Y/a+RED75DDbAQ6lS3fV
            sSlmQLExcf75qOPy34XNv3gWP4tbfIXXt8olflF8hwHggmKZzEImnzEozPabDsN7
            nkhHZEWhGcPRcuHbFOqcirV1sfsKK1gOsTbxS00iD3OivOFCQqujF196cal/utTd
            hVnssTC1arrx273zFepLosPvgrT0TS7tnyXbzuq5mo0zD1fSj4kuSS9V/SSy9fWF
            LAtHiNQJkjzGFxu0/9dyQyX6C523uvfdcOzpObTyjBeGKqmEEf0lF5OYLDlkk2Sm
            iGa6i2oLaGzGaQZDpdqyQZiYpQEYw9xN+8g=
            =J31U
            -----END PGP PUBLIC KEY BLOCK-----
            """
        "# })
        .unwrap()
        .as_table()
        .clone()
    }

    fn create_io_error(text: &str) -> std::io::Error {
        std::io::Error::other(text)
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
