use crate::aptfile::Aptfile;
use crate::AptBuildpack;
use commons::output::interface::SectionLogger;
use commons::output::section_log::log_step;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::Buildpack;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) struct InstalledPackagesLayer<'a> {
    pub(crate) aptfile: &'a Aptfile,
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
        _layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        log_step("Installing packages from Aptfile");

        LayerResultBuilder::new(InstalledPackagesMetadata::new(
            self.aptfile.clone(),
            context.target.os.clone(),
            context.target.arch.clone(),
        ))
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
            log_step("Skipping installation, packages already in cache");
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
                "amd64".to_string(),
            )
            .changed_fields(&InstalledPackagesMetadata::new(
                Aptfile::from_str("package-2").unwrap(),
                "windows".to_string(),
                "arm64".to_string(),
            )),
            &["Aptfile", "arch", "os"]
        );
    }

    #[test]
    fn installed_packages_metadata_with_no_changed_fields() {
        assert!(InstalledPackagesMetadata::new(
            Aptfile::from_str("package-1").unwrap(),
            "linux".to_string(),
            "amd64".to_string(),
        )
        .changed_fields(&InstalledPackagesMetadata::new(
            Aptfile::from_str("package-1").unwrap(),
            "linux".to_string(),
            "amd64".to_string(),
        ))
        .is_empty());
    }

    #[test]
    fn test_metadata_guard() {
        let metadata = InstalledPackagesMetadata::new(
            Aptfile::from_str("package-1").unwrap(),
            "linux".to_string(),
            "amd64".to_string(),
        );
        let actual = toml::to_string(&metadata).unwrap();
        let expected = r#"
arch = "amd64"
os = "linux"

[aptfile]
packages = ["package-1"]
"#
        .trim();
        assert_eq!(expected, actual.trim());
        let from_toml: InstalledPackagesMetadata = toml::from_str(&actual).unwrap();
        assert_eq!(metadata, from_toml);
    }
}
