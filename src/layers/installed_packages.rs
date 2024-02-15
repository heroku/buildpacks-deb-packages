use crate::aptfile::Aptfile;
use crate::AptBuildpack;
use commons::output::interface::SectionLogger;
use commons::output::section_log::log_step;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::Buildpack;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct InstalledPackagesLayer<'a> {
    pub(crate) aptfile: &'a Aptfile,
    pub(crate) cache_restored: &'a AtomicBool,
    pub(crate) _section_logger: &'a dyn SectionLogger,
}

impl<'a> Layer for InstalledPackagesLayer<'a> {
    type Buildpack = AptBuildpack;
    type Metadata = InstalledPackagesMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: true,
            launch: true,
            cache: true,
        }
    }

    fn create(
        &mut self,
        context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        log_step("Creating cache directory");

        let mut env = LayerEnv::new();

        let bin_paths = [
            layer_path.join("bin"),
            layer_path.join("usr/bin"),
            layer_path.join("usr/sbin"),
        ];
        prepend_to_env_var(&mut env, "PATH", ":", &bin_paths);

        let library_paths = [
            layer_path.join("usr/lib/x86_64-linux-gnu"),
            layer_path.join("usr/lib/i386-linux-gnu"),
            layer_path.join("usr/lib"),
            layer_path.join("lib/x86_64-linux-gnu"),
            layer_path.join("lib/i386-linux-gnu"),
            layer_path.join("lib"),
        ];
        prepend_to_env_var(&mut env, "LD_LIBRARY_PATH", ":", &library_paths);
        prepend_to_env_var(&mut env, "LIBRARY_PATH", ":", &library_paths);

        let include_paths = [
            layer_path.join("usr/include/x86_64-linux-gnu"),
            layer_path.join("usr/include/i386-linux-gnu"),
            layer_path.join("usr/include"),
        ];
        prepend_to_env_var(&mut env, "INCLUDE_PATH", ":", &include_paths);
        prepend_to_env_var(&mut env, "CPATH", ":", &include_paths);
        prepend_to_env_var(&mut env, "CPPPATH", ":", &include_paths);

        let pkg_config_paths = [
            layer_path.join("usr/lib/x86_64-linux-gnu/pkgconfig"),
            layer_path.join("usr/lib/i386-linux-gnu/pkgconfig"),
            layer_path.join("usr/lib/pkgconfig"),
        ];
        prepend_to_env_var(&mut env, "PKG_CONFIG_PATH", ":", &pkg_config_paths);

        LayerResultBuilder::new(InstalledPackagesMetadata::new(
            self.aptfile.clone(),
            context.target.os.clone(),
            context.target.arch.clone(),
        ))
        .env(env)
        .build()
    }

    fn existing_layer_strategy(
        &mut self,
        context: &BuildContext<Self::Buildpack>,
        layer_data: &LayerData<Self::Metadata>,
    ) -> Result<ExistingLayerStrategy, <Self::Buildpack as Buildpack>::Error> {
        let old_meta = &layer_data.content_metadata.metadata;
        let new_meta = &InstalledPackagesMetadata::new(
            self.aptfile.clone(),
            context.target.os.clone(),
            context.target.arch.clone(),
        );
        if old_meta == new_meta {
            log_step("Restoring installed packages");
            self.cache_restored.store(true, Ordering::Relaxed);
            Ok(ExistingLayerStrategy::Keep)
        } else {
            log_step(format!(
                "Invalidating installed packages ({} changed)",
                new_meta.changed_fields(old_meta).join(", ")
            ));
            Ok(ExistingLayerStrategy::Recreate)
        }
    }
}

fn prepend_to_env_var<I, T>(env: &mut LayerEnv, name: &str, separator: &str, paths: I)
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    env.insert(Scope::All, ModificationBehavior::Delimiter, name, separator);
    for path in paths {
        env.insert(Scope::All, ModificationBehavior::Prepend, name, path.into());
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct InstalledPackagesMetadata {
    arch: String,
    aptfile: Aptfile,
    os: String,
}

impl InstalledPackagesMetadata {
    pub(crate) fn new(aptfile: Aptfile, os: String, arch: String) -> Self {
        Self { arch, aptfile, os }
    }

    pub(crate) fn changed_fields(&self, other: &InstalledPackagesMetadata) -> Vec<String> {
        let mut changed_fields = vec![];
        if self.os != other.os {
            changed_fields.push("os".to_string());
        }
        if self.arch != other.arch {
            changed_fields.push("arch".to_string());
        }
        if self.aptfile != other.aptfile {
            changed_fields.push("Aptfile".to_string());
        }
        changed_fields.sort();
        changed_fields
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum InstalledPackagesState {
    New(PathBuf),
    Restored,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn installed_packages_metadata_with_all_changed_fields() {
        assert_eq!(
            InstalledPackagesMetadata::new(
                Aptfile::from_str("package-1").unwrap(),
                "linux".to_string(),
                "amd64".to_string()
            )
            .changed_fields(&InstalledPackagesMetadata::new(
                Aptfile::from_str("package-2").unwrap(),
                "windows".to_string(),
                "arm64".to_string()
            )),
            &["Aptfile", "arch", "os"]
        );
    }

    #[test]
    fn installed_packages_metadata_with_no_changed_fields() {
        assert!(InstalledPackagesMetadata::new(
            Aptfile::from_str("package-1").unwrap(),
            "linux".to_string(),
            "amd64".to_string()
        )
        .changed_fields(&InstalledPackagesMetadata::new(
            Aptfile::from_str("package-1").unwrap(),
            "linux".to_string(),
            "amd64".to_string()
        ))
        .is_empty());
    }
}
