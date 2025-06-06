use crate::config::{BuildpackConfig, ConfigError, NAMESPACED_CONFIG};
use crate::create_package_index::{create_package_index, CreatePackageIndexError};
use crate::debian::{Distro, UnsupportedDistroError};
use crate::determine_packages_to_install::{
    determine_packages_to_install, DeterminePackagesToInstallError,
};
use crate::install_packages::{install_packages, InstallPackagesError};
use crate::o11y::*;
use bullet_stream::{global::print, style};
use indoc::formatdoc;
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::{buildpack_main, Buildpack, Env};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use std::fmt::Debug;
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

#[cfg(test)]
use libcnb_test as _;
#[cfg(test)]
use regex as _;

mod config;
mod create_package_index;
mod debian;
mod determine_packages_to_install;
mod errors;
mod install_packages;
mod o11y;
mod pgp;

buildpack_main!(DebianPackagesBuildpack);

type BuildpackResult<T> = Result<T, libcnb::Error<DebianPackagesBuildpackError>>;

struct DebianPackagesBuildpack;

impl Buildpack for DebianPackagesBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = DebianPackagesBuildpackError;

    fn detect(&self, context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        if let Some(project_toml) = get_project_toml(&context.app_dir)? {
            info!({ PROJECT_TOML_DETECTED } = true);
            if BuildpackConfig::is_present(project_toml)? {
                DetectResultBuilder::pass().build()
            } else {
                print::plain("project.toml found, but no [com.heroku.buildpacks.deb-packages] configuration present.");
                info!({ PROJECT_TOML_NO_CONFIG } = true);
                DetectResultBuilder::fail().build()
            }
        } else if get_aptfile(&context.app_dir)?.is_some() {
            // NOTE: This buildpack doesn't use an Aptfile, but we'll pass detection to display a message
            //       to users in the build step detailing how to migrate away from the Aptfile format.
            info!({ APTFILE_DETECTED } = true);
            DetectResultBuilder::pass().build()
        } else {
            print::plain("No project.toml or Aptfile found.");
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        // This buildpack does a lot of async work, so the context needs to be sharable
        // across async boundaries.
        let context = Arc::new(context);

        let client = ClientBuilder::new(
            Client::builder()
                .use_rustls_tls()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(10))
                .build()
                .expect("Should be able to construct the HTTP Client"),
        )
        .with(RetryTransientMiddleware::new_with_policy(
            ExponentialBackoff::builder().build_with_max_retries(5),
        ))
        .with(TracingMiddleware::<SpanBackendWithUrl>::new())
        .build();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("Should be able to construct the Async Runtime");

        let started = print::buildpack(format!(
            "{buildpack_name} (v{buildpack_version})",
            buildpack_name = context
                .buildpack_descriptor
                .buildpack
                .name
                .as_ref()
                .expect("buildpack name should be set"),
            buildpack_version = context.buildpack_descriptor.buildpack.version
        ));

        if get_aptfile(&context.app_dir)?.is_some() {
            print::plain(style::important(migrate_from_aptfile_help_message()));
            // If we passed detect from the Aptfile but there is no project.toml then
            // print the warning and exit early.
            if get_project_toml(&context.app_dir)?.is_none() {
                info!({ EARLY_EXIT_REASON } = "migrate_aptfile", "early exit");
                return BuildResultBuilder::new().build();
            }
        }

        let config = BuildpackConfig::try_from(context.app_dir.join("project.toml"))?;

        if config.install.is_empty() {
            info!({ EARLY_EXIT_REASON } = "nothing_to_install", "early exit");

            print::plain(style::important(empty_config_help_message()));
            return BuildResultBuilder::new().build();
        }

        let distro = Distro::try_from(&context.target)?;

        let source_list = distro.get_source_list();

        info!(
            { DISTRO_NAME } = %distro.name,
            { DISTRO_VERSION } =  %distro.version,
            { DISTRO_CODENAME } = %distro.codename,
            { DISTRO_ARCH } = %distro.architecture,
            { SOURCE_LIST } = as_json_value(&source_list),
            { CONFIG_INSTALL } = as_json_value(&config.install.iter().collect::<Vec<_>>()),
            "configuration"
        );

        print::bullet("Distribution Info");
        print::sub_bullet(format!("Name: {}", &distro.name));
        print::sub_bullet(format!("Version: {}", &distro.version));
        print::sub_bullet(format!("Codename: {}", &distro.codename));
        print::sub_bullet(format!("Architecture: {}", &distro.architecture));

        let package_index =
            runtime.block_on(create_package_index(&context, &client, &source_list))?;

        let packages_to_install = determine_packages_to_install(&package_index, config.install)?;

        runtime.block_on(install_packages(
            &context,
            &client,
            &distro,
            packages_to_install,
        ))?;

        print::all_done(&Some(started));

        BuildResultBuilder::new().build()
    }

    fn on_error(&self, error: libcnb::Error<Self::Error>) {
        error!({ ERROR } = ?error);
        errors::on_error(error, stdout());
    }
}

#[derive(Debug)]
pub(crate) enum DebianPackagesBuildpackError {
    Config(ConfigError),
    UnsupportedDistro(UnsupportedDistroError),
    CreatePackageIndex(CreatePackageIndexError),
    DeterminePackagesToInstall(Box<DeterminePackagesToInstallError>),
    InstallPackages(Box<InstallPackagesError>),
    Detect(DetectError),
}

impl From<DebianPackagesBuildpackError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: DebianPackagesBuildpackError) -> Self {
        Self::BuildpackError(value)
    }
}

#[derive(Debug)]
pub(crate) enum DetectError {
    CheckExistsAptfile(PathBuf, std::io::Error),
    CheckExistsProjectToml(PathBuf, std::io::Error),
}

impl From<DetectError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: DetectError) -> Self {
        Self::BuildpackError(DebianPackagesBuildpackError::Detect(value))
    }
}

pub(crate) fn is_buildpack_debug_logging_enabled() -> bool {
    Env::from_current()
        .get("BP_LOG_LEVEL")
        .is_some_and(|value| value.eq_ignore_ascii_case("debug"))
}

fn get_aptfile(app_dir: &Path) -> Result<Option<PathBuf>, DetectError> {
    let aptfile = app_dir.join("Aptfile");
    aptfile
        .try_exists()
        .map_err(|e| DetectError::CheckExistsAptfile(aptfile.clone(), e))
        .map(|exists| if exists { Some(aptfile) } else { None })
}

fn get_project_toml(app_dir: &Path) -> Result<Option<PathBuf>, DetectError> {
    let project_toml = app_dir.join("project.toml");
    project_toml
        .try_exists()
        .map_err(|e| DetectError::CheckExistsProjectToml(project_toml.clone(), e))
        .map(|exists| if exists { Some(project_toml) } else { None })
}

fn empty_config_help_message() -> String {
    formatdoc! {"
        No configured packages to install found in project.toml file. You may need to \
        add a list of packages to install in your project.toml like this:

        [{NAMESPACED_CONFIG}]
        install = [
            \"package-name\",
        ]
    " }
    .trim()
    .to_string()
}

fn migrate_from_aptfile_help_message() -> String {
    let aptfile = style::value("Aptfile");
    let apt_buildpack_name = style::value("heroku-community/apt");
    let project_toml = style::value("project.toml");
    let configuration_readme_url = style::url(
        "https://github.com/heroku/buildpacks-deb-packages?tab=readme-ov-file#configuration",
    );
    formatdoc! { "
        The use of an {aptfile} is deprecated!

        Users of the {apt_buildpack_name} buildpack can migrate their installed packages to be compatible \
        with this buildpack's configuration by adding a {project_toml} file with:

            [_]
            schema-version = \"0.2\"

            [com.heroku.buildpacks.deb-packages]
            install = [
                # copy the contents of your Aptfile here, e.g.;
                # \"package-a\",
                # \"package-b\",
                # \"package-c\"
            ]

        If your {aptfile} contains a package name that uses wildcards (e.g.; mysql-*) this must be replaced \
        with the full list of matching package names. See {configuration_readme_url}
    " }
    .trim()
    .to_string()
}
