use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use commons::output::fmt;
use commons::output::interface::SectionLogger;
use commons::output::section_log::{log_step, log_step_stream, log_step_timed};
use fun_run::CommandWithName;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::Buildpack;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::aptfile::Aptfile;
use crate::commands::apt::{AptGetCommand, AptVersion};
use crate::commands::dpkg::DpkgCommand;
use crate::debian::DebianPackageName;
use crate::dependency_versions::DependencyVersions;
use crate::errors::AptBuildpackError;
use crate::AptBuildpack;

pub(crate) struct InstalledPackagesLayer<'a> {
    pub(crate) aptfile: &'a Aptfile,
    pub(crate) apt_dir: &'a TempDir,
    pub(crate) apt_config: &'a PathBuf,
    pub(crate) apt_version: &'a AptVersion,
    pub(crate) dependency_versions: &'a DependencyVersions,
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
        log_step(format!(
            "Installing packages from Aptfile (apt-get version {})",
            fmt::value(self.apt_version.to_string())
        ));
        download_packages(&self.aptfile.packages, self.apt_config, self.apt_version)?;

        log_step_timed(
            format!("Extracting packages to {}", layer_path.to_string_lossy()),
            || {
                fs::read_dir(self.apt_dir.path().join("cache/archives"))
                    .map_err(AptBuildpackError::ListDownloadedPackages)?
                    .flatten()
                    .filter(|entry| entry.path().extension() == Some("deb".as_ref()))
                    .try_for_each(|downloaded_package| {
                        extract_package(&downloaded_package.path(), layer_path)
                    })
            },
        )?;

        LayerResultBuilder::new(InstalledPackagesMetadata::new(
            self.aptfile.clone(),
            context.target.os.clone(),
            context.target.arch.clone(),
            self.dependency_versions.clone(),
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
            self.dependency_versions.clone(),
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

fn download_packages(
    packages: &HashSet<DebianPackageName>,
    config_file: &Path,
    apt_version: &AptVersion,
) -> Result<(), AptBuildpackError> {
    let mut apt_get = AptGetCommand::new();
    apt_get.config_file = Some(config_file.to_path_buf());
    apt_get.assume_yes = true;
    apt_get.download_only = true;
    apt_get.reinstall = true;

    let force_yes_requirement =
        semver::VersionReq::parse("<1.1").expect("this should be a valid semver range");

    if force_yes_requirement.matches(apt_version) {
        apt_get.force_yes = true;
    } else {
        apt_get.allow_downgrades = true;
        apt_get.allow_remove_essential = true;
        apt_get.allow_change_held_packages = true;
    };

    let mut apt_get_install = apt_get.install();
    for package in packages {
        apt_get_install.packages.insert(package.clone());
    }

    let mut command = Command::from(apt_get_install);
    log_step_stream(
        format!("Downloading packages with {}", fmt::command(command.name())),
        |stream| command.stream_output(stream.io(), stream.io()),
    )
    .map_err(AptBuildpackError::DownloadPackages)
    .map(|_| ())
}

fn extract_package(package_archive: &Path, install_dir: &Path) -> Result<(), AptBuildpackError> {
    let mut dpkg = DpkgCommand::new();
    dpkg.extract(package_archive.to_path_buf(), install_dir.to_path_buf());
    Command::from(dpkg)
        .named_output()
        .map_err(|e| AptBuildpackError::InstallPackage(package_archive.to_path_buf(), e))
        .map(|_| ())
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstalledPackagesMetadata {
    arch: String,
    aptfile: Aptfile,
    os: String,
    dependency_versions: DependencyVersions,
}

impl InstalledPackagesMetadata {
    pub(crate) fn new(
        aptfile: Aptfile,
        os: String,
        arch: String,
        dependency_versions: DependencyVersions,
    ) -> Self {
        Self {
            arch,
            aptfile,
            os,
            dependency_versions,
        }
    }

    pub(crate) fn changed_fields(&self, other: &InstalledPackagesMetadata) -> Vec<String> {
        let mut changed_fields = vec![];
        if self.os != other.os {
            changed_fields.push("os".to_string());
        }
        if self.arch != other.arch {
            changed_fields.push("arch".to_string());
        }
        // (changes to packages or new versions available are basically the same thing)
        if self.aptfile != other.aptfile || self.dependency_versions != other.dependency_versions {
            changed_fields.push("packages".to_string());
        }
        changed_fields.sort();
        changed_fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_versions::CandidateVersion;

    #[test]
    fn installed_packages_metadata_with_all_changed_fields() {
        assert_eq!(
            InstalledPackagesMetadata::new(
                Aptfile {
                    packages: HashSet::from([DebianPackageName("package-1".to_string())]),
                },
                "linux".to_string(),
                "amd64".to_string(),
                DependencyVersions::from([(
                    DebianPackageName("package-1".to_string()),
                    CandidateVersion("1.2.3".to_string()),
                )]),
            )
            .changed_fields(&InstalledPackagesMetadata::new(
                Aptfile {
                    packages: HashSet::from([DebianPackageName("package-2".to_string())]),
                },
                "windows".to_string(),
                "arm64".to_string(),
                DependencyVersions::from([(
                    DebianPackageName("package-2".to_string()),
                    CandidateVersion("2.3.4".to_string()),
                )]),
            )),
            &["arch", "os", "packages"]
        );
    }

    #[test]
    fn installed_packages_metadata_with_aptfile_same_but_dependency_versions_changed() {
        assert_eq!(
            InstalledPackagesMetadata::new(
                Aptfile {
                    packages: HashSet::from([DebianPackageName("package-1".to_string())]),
                },
                "linux".to_string(),
                "amd64".to_string(),
                DependencyVersions::from([(
                    DebianPackageName("package-1".to_string()),
                    CandidateVersion("1.2.3".to_string()),
                )]),
            )
            .changed_fields(&InstalledPackagesMetadata::new(
                Aptfile {
                    packages: HashSet::from([DebianPackageName("package-1".to_string())]),
                },
                "linux".to_string(),
                "amd64".to_string(),
                DependencyVersions::from([(
                    DebianPackageName("package-1".to_string()),
                    CandidateVersion("1.2.4".to_string()),
                )]),
            )),
            &["packages"]
        );
    }

    #[test]
    fn installed_packages_metadata_with_no_changed_fields() {
        assert!(InstalledPackagesMetadata::new(
            Aptfile {
                packages: HashSet::from([DebianPackageName("package-1".to_string())]),
            },
            "linux".to_string(),
            "amd64".to_string(),
            DependencyVersions::from([(
                DebianPackageName("package-1".to_string()),
                CandidateVersion("1.2.3".to_string()),
            )]),
        )
        .changed_fields(&InstalledPackagesMetadata::new(
            Aptfile {
                packages: HashSet::from([DebianPackageName("package-1".to_string())]),
            },
            "linux".to_string(),
            "amd64".to_string(),
            DependencyVersions::from([(
                DebianPackageName("package-1".to_string()),
                CandidateVersion("1.2.3".to_string()),
            )]),
        ))
        .is_empty());
    }

    #[test]
    fn test_metadata_guard() {
        let metadata = InstalledPackagesMetadata::new(
            Aptfile {
                packages: HashSet::from([DebianPackageName("package-1".to_string())]),
            },
            "linux".to_string(),
            "amd64".to_string(),
            DependencyVersions::from([(
                DebianPackageName("package-1".to_string()),
                CandidateVersion("1.2.3".to_string()),
            )]),
        );
        let actual = toml::to_string(&metadata).unwrap();
        let expected = r#"
arch = "amd64"
os = "linux"

[aptfile]
packages = ["package-1"]

[dependency_versions]
package-1 = "1.2.3"
"#
        .trim();
        assert_eq!(expected, actual.trim());
        let from_toml: InstalledPackagesMetadata = toml::from_str(&actual).unwrap();
        assert_eq!(metadata, from_toml);
    }
}
