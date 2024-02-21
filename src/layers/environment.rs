use crate::debian::{DebianArchitectureName, DebianMultiarchName};
use crate::AptBuildpack;
use commons::output::build_log::SectionLogger;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::generic::GenericMetadata;
use libcnb::layer::{Layer, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::Buildpack;
use std::ffi::OsString;
use std::path::Path;

pub(crate) struct EnvironmentLayer<'a> {
    pub(crate) debian_architecture_name: &'a DebianArchitectureName,
    pub(crate) installed_packages_dir: &'a Path,
    pub(crate) _section_logger: &'a dyn SectionLogger,
}

impl<'a> Layer for EnvironmentLayer<'a> {
    type Buildpack = AptBuildpack;
    type Metadata = GenericMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: true,
            launch: true,
            cache: false,
        }
    }

    fn create(
        &mut self,
        _context: &BuildContext<Self::Buildpack>,
        _layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        LayerResultBuilder::new(GenericMetadata::default())
            .env(configure_environment(
                self.installed_packages_dir,
                &DebianMultiarchName::from(self.debian_architecture_name),
            ))
            .build()
    }
}

fn configure_environment(
    packages_dir: &Path,
    debian_multiarch_name: &DebianMultiarchName,
) -> LayerEnv {
    let mut env = LayerEnv::new();

    let bin_paths = [
        packages_dir.join("bin"),
        packages_dir.join("usr/bin"),
        packages_dir.join("usr/sbin"),
    ];
    prepend_to_env_var(&mut env, "PATH", &bin_paths);

    // support multi-arch and legacy filesystem layouts for debian packages
    // https://wiki.ubuntu.com/MultiarchSpec
    let library_paths = [
        packages_dir.join(format!("usr/lib/{debian_multiarch_name}")),
        packages_dir.join("usr/lib"),
        packages_dir.join(format!("lib/{debian_multiarch_name}")),
        packages_dir.join("lib"),
    ];
    prepend_to_env_var(&mut env, "LD_LIBRARY_PATH", &library_paths);
    prepend_to_env_var(&mut env, "LIBRARY_PATH", &library_paths);

    let include_paths = [
        packages_dir.join(format!("usr/include/{debian_multiarch_name}")),
        packages_dir.join("usr/include"),
    ];
    prepend_to_env_var(&mut env, "INCLUDE_PATH", &include_paths);
    prepend_to_env_var(&mut env, "CPATH", &include_paths);
    prepend_to_env_var(&mut env, "CPPPATH", &include_paths);

    let pkg_config_paths = [
        packages_dir.join(format!("usr/lib/{debian_multiarch_name}/pkgconfig")),
        packages_dir.join("usr/lib/pkgconfig"),
    ];
    prepend_to_env_var(&mut env, "PKG_CONFIG_PATH", &pkg_config_paths);

    env
}

fn prepend_to_env_var<I, T>(env: &mut LayerEnv, name: &str, paths: I)
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let separator = ":";
    env.insert(Scope::All, ModificationBehavior::Delimiter, name, separator);
    env.insert(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_configure_environment() {
        let debian_multiarch_name = DebianMultiarchName::X86_64_LINUX_GNU;
        let layer_env = configure_environment(&PathBuf::from("/"), &debian_multiarch_name);
        let env = layer_env.apply_to_empty(Scope::All);
        assert_eq!(env.get("PATH").unwrap(), "/bin:/usr/bin:/usr/sbin");
        assert_eq!(
            env.get("LD_LIBRARY_PATH").unwrap(),
            "/usr/lib/x86_64-linux-gnu:/usr/lib:/lib/x86_64-linux-gnu:/lib"
        );
        assert_eq!(
            env.get("LIBRARY_PATH").unwrap(),
            "/usr/lib/x86_64-linux-gnu:/usr/lib:/lib/x86_64-linux-gnu:/lib"
        );
        assert_eq!(
            env.get("INCLUDE_PATH").unwrap(),
            "/usr/include/x86_64-linux-gnu:/usr/include"
        );
        assert_eq!(
            env.get("CPATH").unwrap(),
            "/usr/include/x86_64-linux-gnu:/usr/include"
        );
        assert_eq!(
            env.get("CPPPATH").unwrap(),
            "/usr/include/x86_64-linux-gnu:/usr/include"
        );
        assert_eq!(
            env.get("PKG_CONFIG_PATH").unwrap(),
            "/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig"
        );
    }
}
