use const_format::formatcp;
use serde::Serialize;

const NAMESPACE: &str = "cnb.deb_packages";

/// Indicates whether a project.toml file was detected in the application root
/// Useful for understanding if users are using project.toml for configuration vs. a legacy Aptfile they'll need to migrate to project.toml
pub(crate) const PROJECT_TOML_DETECTED: &str = formatcp!("{NAMESPACE}.project_toml.detected");

/// Indicates that a project.toml was found but contained no relevant configuration for the buildpack
/// Helps identify potential configuration issues
pub(crate) const PROJECT_TOML_NO_CONFIG: &str = formatcp!("{NAMESPACE}.project.toml.no_config");

/// Indicates whether an Aptfile was detected in the build context
/// Useful for understanding if users are using a legacy Aptfile vs. project.toml
pub(crate) const APTFILE_DETECTED: &str = formatcp!("{NAMESPACE}.aptfile.detected");

/// Captures the reason for early exit from the build process
/// Helps identify buildpack misconfigurations for a user's application
pub(crate) const EARLY_EXIT_REASON: &str = formatcp!("{NAMESPACE}.early_exit.reason");

const DISTRO: &str = formatcp!("{NAMESPACE}.distro");

/// The name of the Linux distribution being used (e.g., "Ubuntu", "Debian")
/// Important for understanding the build environment and potential compatibility issues
pub(crate) const DISTRO_NAME: &str = formatcp!("{DISTRO}.name");

/// The version of the Linux distribution being used
/// Helps track compatibility and identify version-specific issues
pub(crate) const DISTRO_VERSION: &str = formatcp!("{DISTRO}.version");

/// The codename of the Linux distribution (e.g., "jammy", "noble")
/// Useful for precise identification of the distribution release
pub(crate) const DISTRO_CODENAME: &str = formatcp!("{NAMESPACE}.codename");

/// The system architecture being used (e.g., "amd64", "arm64")
/// Critical for understanding package compatibility and build requirements
pub(crate) const DISTRO_ARCH: &str = formatcp!("{NAMESPACE}.architecture");

/// The source list configuration used for package repositories
/// Helps track which distribution package sources are being used and potential issues
pub(crate) const SOURCE_LIST: &str = formatcp!("{NAMESPACE}.source_list");

/// User requested packages for installation
/// Useful for understanding what packages are commonly requested by users that may be candidates for inclusion in the base image
pub(crate) const CONFIG_INSTALL: &str = formatcp!("{NAMESPACE}.config.install");

const RELEASE: &str = formatcp!("{NAMESPACE}.release");

/// The URI of the distribution release
/// Helps track release file source and potential connectivity issues when requesting release files
pub(crate) const RELEASE_URI: &str = formatcp!("{RELEASE}.uri");

/// The suite name of the distribution release (e.g., "jammy", "jammy-security")
/// Indicates the suite used when requesting release files
pub(crate) const RELEASE_SUITE: &str = formatcp!("{RELEASE}.suite");

const PACKAGE_LIST: &str = formatcp!("{NAMESPACE}.package_list");

/// The URI of the package list
/// Helps track package list source and potential connectivity issues when requesting package lists
pub(crate) const PACKAGE_LIST_URI: &str = formatcp!("{PACKAGE_LIST}.uri");

/// The suite name for the package list
/// Helps track which suite is being requested when updating package lists
pub(crate) const PACKAGE_LIST_SUITE: &str = formatcp!("{PACKAGE_LIST}.suite");

/// The component name in the package list (e.g., "main", "universe")
/// Helps track which package components are being requested when updating package lists
pub(crate) const PACKAGE_LIST_COMPONENT: &str = formatcp!("{PACKAGE_LIST}.component");

/// The architecture in the package list
/// Helps track which architecture is being requested when updating package lists
pub(crate) const PACKAGE_LIST_ARCH: &str = formatcp!("{PACKAGE_LIST}.arch");

/// Whether the package list uses hash-based urls
/// Helps track which url type is being used when requesting package lists
pub(crate) const PACKAGE_LIST_ACQUIRE_BY_HASH: &str = formatcp!("{PACKAGE_LIST}.acquire_by_hash");

/// The number of packages in the package list
/// Useful for getting a sense of the size of various package lists
pub(crate) const PACKAGE_LIST_SIZE: &str = formatcp!("{PACKAGE_LIST}.size");

/// The total size of all packages from all package lists in the package index
/// Useful for getting a sense of the size of the package index
pub(crate) const PACKAGE_INDEX_SIZE: &str = formatcp!("{NAMESPACE}.package_index.size");

/// List of packages that will be installed after resolving dependencies from the package index
/// Helps track which packages are being installed after resolving dependencies from the package index
pub(crate) const PACKAGES_TO_INSTALL: &str = formatcp!("{NAMESPACE}.packages_to_install");

const DOWNLOAD_PACKAGE: &str = formatcp!("{NAMESPACE}.download_package");

/// The name of the package being downloaded
/// Helps track individual package downloads and potential issues
pub(crate) const DOWNLOAD_PACKAGE_NAME: &str = formatcp!("{DOWNLOAD_PACKAGE}.name");

/// The version of the package being downloaded
/// Important for version tracking and compatibility verification
pub(crate) const DOWNLOAD_PACKAGE_VERSION: &str = formatcp!("{DOWNLOAD_PACKAGE}.version");

/// The decoder being used for package extraction (e.g. "gzip", "xz", "zstd")
/// Helps track package format and extraction method
pub(crate) const EXTRACT_PACKAGE_DECODER: &str = formatcp!("{NAMESPACE}.extract_package.decoder");

const ENV: &str = formatcp!("{NAMESPACE}.env");

/// The `PATH` environment variable value exported by the buildpack
/// Critical for understanding executable search paths
pub(crate) const ENV_PATH: &str = formatcp!("{ENV}.path");

/// The `LD_LIBRARY_PATH` environment variable value exported by the buildpack
/// Important for library loading and runtime behavior
pub(crate) const LIBRARY_PATH: &str = formatcp!("{ENV}.library_path");

/// The include path environment variable value exported by the buildpack
/// Helps track header file locations for compilation
pub(crate) const INCLUDE_PATH: &str = formatcp!("{ENV}.include_path");

/// The `PKG_CONFIG_PATH` environment variable value exported by the buildpack
/// Important for package configuration and build system integration
pub(crate) const PKG_CONFIG_PATH: &str = formatcp!("{ENV}.pkg_config_path");

/// Captures error information during the build process
/// Critical for debugging and understanding build failures
pub(crate) const ERROR: &str = formatcp!("{NAMESPACE}.error");

pub(crate) fn as_json_value<T>(value: &T) -> String
where
    T: Serialize,
{
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("Failed to serialize JSON: {e}"))
}
