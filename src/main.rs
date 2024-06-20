use std::fmt::Debug;
use std::fs;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;

#[cfg(test)]
use libcnb_test as _;
#[cfg(test)]
use regex as _;

use crate::config::{BuildpackConfig, ParseConfigError};
use crate::create_package_index::{create_package_index, CreatePackageIndexError};
use crate::debian::{SupportedDistro, UnsupportedDistroError};
use crate::determine_packages_to_install::{
    determine_packages_to_install, DeterminePackagesToInstallError,
};
use crate::install_packages::{install_packages, InstallPackagesError};
use crate::DebianPackagesBuildpackError::{
    CreateAsyncRuntime, CreateHttpClient, CreatePackageIndex, DetectFailed, InstallPackages,
    ParseConfig, ReadRequestedPackages, SolvePackagesToInstall, UnsupportedDistro,
};

mod config;
mod create_package_index;
mod debian;
mod determine_packages_to_install;
mod install_packages;
mod on_package_install;
mod pgp;

// Include buildpack information from `build.rs`.
include!(concat!(env!("OUT_DIR"), "/buildpack_info.rs"));

buildpack_main!(DebianPackagesBuildpack);

#[derive(Debug)]
#[allow(dead_code)] // TODO: remove this once error messages are added
pub(crate) enum DebianPackagesBuildpackError {
    DetectFailed(std::io::Error),
    ParseConfig(ParseConfigError),
    ReadRequestedPackages(std::io::Error),
    CreateHttpClient(reqwest::Error),
    CreateAsyncRuntime(std::io::Error),
    UnsupportedDistro(UnsupportedDistroError),
    CreatePackageIndex(CreatePackageIndexError),
    SolvePackagesToInstall(DeterminePackagesToInstallError),
    InstallPackages(InstallPackagesError),
}

impl From<DebianPackagesBuildpackError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: DebianPackagesBuildpackError) -> Self {
        Self::BuildpackError(value)
    }
}

struct DebianPackagesBuildpack;

impl Buildpack for DebianPackagesBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = DebianPackagesBuildpackError;

    fn detect(&self, context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        let project_toml = context.app_dir.join("project.toml");

        let project_file_exists = project_toml.try_exists().map_err(DetectFailed)?;

        if project_file_exists {
            let config = fs::read_to_string(project_toml)
                .map_err(DetectFailed)
                .and_then(|contents| BuildpackConfig::from_str(&contents).map_err(ParseConfig))?;
            if config.install.is_empty() {
                println!("No configured packages to install found in project.toml file.");
                DetectResultBuilder::fail().build()
            } else {
                DetectResultBuilder::pass().build()
            }
        } else {
            println!("No project.toml file found.");
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        println!("# {BUILDPACK_NAME} (v{BUILDPACK_VERSION})");
        println!();

        let config = fs::read_to_string(context.app_dir.join("project.toml"))
            .map_err(ReadRequestedPackages)
            .and_then(|contents| BuildpackConfig::from_str(&contents).map_err(ParseConfig))?;

        let distro = SupportedDistro::try_from(&context.target).map_err(UnsupportedDistro)?;

        let shared_context = Arc::new(context);

        let client = ClientBuilder::new(
            Client::builder()
                .use_rustls_tls()
                .timeout(Duration::from_secs(60 * 10))
                .build()
                .map_err(CreateHttpClient)?,
        )
        .with(RetryTransientMiddleware::new_with_policy(
            ExponentialBackoff::builder().build_with_max_retries(5),
        ))
        .build();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(CreateAsyncRuntime)?;

        println!("## Distribution Info");
        println!();
        println!("- Name: {}", &distro.name);
        println!("- Version: {}", &distro.version);
        println!("- Codename: {}", &distro.codename);
        println!("- Architecture: {}", &distro.architecture);
        println!();

        let package_index = runtime
            .block_on(create_package_index(&shared_context, &client, &distro))
            .map_err(CreatePackageIndex)?;

        let packages_to_install = determine_packages_to_install(&package_index, config.install)
            .map_err(SolvePackagesToInstall)?;

        runtime
            .block_on(install_packages(
                &shared_context,
                &client,
                &distro,
                packages_to_install,
            ))
            .map_err(InstallPackages)?;

        BuildResultBuilder::new().build()
    }
}
