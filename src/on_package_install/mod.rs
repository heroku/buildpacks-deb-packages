use std::path::Path;

use libcnb::layer_env::LayerEnv;

use crate::debian::{MultiarchName, RepositoryPackage, SupportedDistro};
use crate::on_package_install::configure_layer_environment::configure_layer_environment;
use crate::on_package_install::rewrite_package_configs::{
    rewrite_package_configs, RewritePackageConfigsError,
};
use crate::on_package_install::OnPackageInstallError::RewritePackageConfigs;

mod configure_layer_environment;
mod rewrite_package_configs;

type Result<T> = std::result::Result<T, OnPackageInstallError>;

pub(crate) fn on_package_install(
    _package: &RepositoryPackage,
    install_path: &Path,
    distro: &SupportedDistro,
) -> Result<LayerEnv> {
    let mut layer_env = LayerEnv::new();

    configure_layer_environment(
        install_path,
        &MultiarchName::from(&distro.architecture),
        &mut layer_env,
    );

    rewrite_package_configs(install_path).map_err(RewritePackageConfigs)?;

    Ok(layer_env)
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum OnPackageInstallError {
    RewritePackageConfigs(RewritePackageConfigsError),
}
