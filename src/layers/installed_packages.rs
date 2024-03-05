use crate::aptfile::Aptfile;
use crate::commands::apt_get::{AptGetCommand, AptVersion};
use crate::commands::dpkg::DpkgCommand;
use crate::debian::DebianPackageName;
use crate::errors::AptBuildpackError;
use crate::AptBuildpack;
use commons::output::fmt;
use commons::output::interface::SectionLogger;
use commons::output::section_log::{log_step, log_step_stream, log_step_timed};
use fun_run::CommandWithName;
use indoc::formatdoc;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::Buildpack;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{tempdir, TempDir};

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
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        let apt_dir = create_apt_dir()?;
        let apt_config = create_apt_config(apt_dir.path())?;
        let apt_version = get_apt_version()?;

        log_step(format!(
            "Installing packages from Aptfile (apt-get version {})",
            fmt::value(apt_version.to_string())
        ));

        update_apt_sources(&apt_config)?;
        download_packages(&self.aptfile.packages, &apt_config, &apt_version)?;

        log_step_timed(
            format!("Extracting packages to {}", layer_path.to_string_lossy()),
            || {
                fs::read_dir(apt_dir.path().join("cache/archives"))
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

fn create_apt_dir() -> Result<TempDir, AptBuildpackError> {
    let apt_dir = tempdir().map_err(AptBuildpackError::CreateAptDir)?;
    // apt-get complains if these folders aren't present
    fs::create_dir_all(apt_dir.path().join("state/lists/partial"))
        .map_err(AptBuildpackError::CreateAptDir)?;
    fs::create_dir_all(apt_dir.path().join("cache/archives/partial"))
        .map_err(AptBuildpackError::CreateAptDir)?;
    Ok(apt_dir)
}

fn create_apt_config(apt_dir: &Path) -> Result<PathBuf, AptBuildpackError> {
    // https://manpages.ubuntu.com/manpages/jammy/man5/apt.conf.5.html
    // set a custom apt.conf so that our calls to apt-get use our custom installation
    let apt_config = apt_dir.join("apt.conf");
    fs::write(
        apt_dir.join("apt.conf"),
        formatdoc! { r#"
            #clear APT::Update::Post-Invoke;
            Debug::NoLocking "true";
            Dir::Cache "{apt_dir}/cache";
            Dir::State "{apt_dir}/state";      
        "#, apt_dir = apt_dir.to_string_lossy() },
    )
    .map_err(AptBuildpackError::CreateAptConfig)
    .map(|()| apt_config)
}

fn get_apt_version() -> Result<AptVersion, AptBuildpackError> {
    Command::new("apt-get")
        .arg("--version")
        .named_output()
        .map_err(AptBuildpackError::AptGetVersionCommand)
        .and_then(|output| {
            output
                .stdout_lossy()
                .parse::<AptVersion>()
                .map_err(AptBuildpackError::ParseAptGetVersion)
        })
}

fn update_apt_sources(config_file: &Path) -> Result<(), AptBuildpackError> {
    let mut command = Command::new("apt-get");
    command.arg("--config-file").arg(config_file).arg("update");

    log_step_stream(
        format!("Updating sources with {}", fmt::command(command.name())),
        |stream| command.stream_output(stream.io(), stream.io()),
    )
    .map_err(AptBuildpackError::AptGetUpdate)
    .map(|_| ())
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
        semver::VersionReq::parse("<=1.0").expect("this should be a valid semver range");

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
