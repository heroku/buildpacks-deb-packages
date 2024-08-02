use std::fmt::Debug;
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

use crate::config::{BuildpackConfig, ConfigError};
use crate::create_package_index::{create_package_index, CreatePackageIndexError};
use crate::debian::{Distro, UnsupportedDistroError};
use crate::determine_packages_to_install::{
    determine_packages_to_install, DeterminePackagesToInstallError,
};
use crate::install_packages::{install_packages, InstallPackagesError};
use crate::DebianPackagesBuildpackError::{
    Config, CreateAsyncRuntime, CreateHttpClient, CreatePackageIndex, DetectConfigFile,
    InstallPackages, SolvePackagesToInstall, UnsupportedDistro,
};

mod config;
mod create_package_index;
mod debian;
mod determine_packages_to_install;
mod install_packages;
mod pgp;

buildpack_main!(DebianPackagesBuildpack);

#[derive(Debug)]
#[allow(dead_code)] // TODO: remove this once error messages are added
pub(crate) enum DebianPackagesBuildpackError {
    DetectConfigFile(std::io::Error),
    Config(ConfigError),
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
        if context
            .app_dir
            .join("project.toml")
            .try_exists()
            .map_err(DetectConfigFile)?
        {
            DetectResultBuilder::pass().build()
        } else {
            println!("No project.toml file found.");
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        println!(
            "# {buildpack_name} (v{buildpack_version})",
            buildpack_name = context
                .buildpack_descriptor
                .buildpack
                .name
                .as_ref()
                .expect("buildpack name should be set"),
            buildpack_version = context.buildpack_descriptor.buildpack.version
        );
        println!();

        let config =
            BuildpackConfig::try_from(context.app_dir.join("project.toml")).map_err(Config)?;

        if config.install.is_empty() {
            println!(
                "{message}",
                message = "
No configured packages to install found in project.toml file. You may need to \
add a list of packages to install in your project.toml like this:

[com.heroku.buildpacks.debian-packages]
install = [
  \"package-name\",
]
            "
                .trim()
            );
            println!();
            return BuildResultBuilder::new().build();
        }

        let distro = Distro::try_from(&context.target).map_err(UnsupportedDistro)?;

        let shared_context = Arc::new(context);

        let client = ClientBuilder::new(
            Client::builder()
                .use_rustls_tls()
                .timeout(Duration::from_secs(60 * 5))
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
