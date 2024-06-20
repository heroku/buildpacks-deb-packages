use std::ffi::OsString;
use std::path::Path;

use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};

use crate::debian::MultiarchName;

pub(crate) fn configure_layer_environment(
    install_path: &Path,
    multiarch_name: &MultiarchName,
    layer_env: &mut LayerEnv,
) {
    let bin_paths = [
        install_path.join("bin"),
        install_path.join("usr/bin"),
        install_path.join("usr/sbin"),
    ];
    prepend_to_env_var(layer_env, "PATH", &bin_paths);

    // support multi-arch and legacy filesystem layouts for debian packages
    // https://wiki.ubuntu.com/MultiarchSpec
    let library_paths = [
        install_path.join(format!("usr/lib/{multiarch_name}")),
        install_path.join("usr/lib"),
        install_path.join(format!("lib/{multiarch_name}")),
        install_path.join("lib"),
    ];
    prepend_to_env_var(layer_env, "LD_LIBRARY_PATH", &library_paths);
    prepend_to_env_var(layer_env, "LIBRARY_PATH", &library_paths);

    let include_paths = [
        install_path.join(format!("usr/include/{multiarch_name}")),
        install_path.join("usr/include"),
    ];
    prepend_to_env_var(layer_env, "INCLUDE_PATH", &include_paths);
    prepend_to_env_var(layer_env, "CPATH", &include_paths);
    prepend_to_env_var(layer_env, "CPPPATH", &include_paths);

    let pkg_config_paths = [
        install_path.join(format!("usr/lib/{multiarch_name}/pkgconfig")),
        install_path.join("usr/lib/pkgconfig"),
    ];
    prepend_to_env_var(layer_env, "PKG_CONFIG_PATH", &pkg_config_paths);
}

fn prepend_to_env_var<I, T>(layer_env: &mut LayerEnv, name: &str, paths: I)
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let separator = ":";
    layer_env.insert(Scope::All, ModificationBehavior::Delimiter, name, separator);
    layer_env.insert(
        Scope::All,
        ModificationBehavior::Prepend,
        name,
        paths
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
            .join(separator.as_ref()),
    );
}
