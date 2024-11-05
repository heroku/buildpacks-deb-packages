use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use indexmap::IndexSet;
use toml_edit::{DocumentMut, TableLike};

use crate::config::{ParseRequestedPackageError, RequestedPackage};
use crate::{BuildpackResult, DebianPackagesBuildpackError};

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct BuildpackConfig {
    pub(crate) install: IndexSet<RequestedPackage>,
}

impl BuildpackConfig {
    pub(crate) fn exists(config_file: impl AsRef<Path>) -> BuildpackResult<bool> {
        Ok(config_file
            .as_ref()
            .try_exists()
            .map_err(|e| ConfigError::CheckExists(config_file.as_ref().to_path_buf(), e))?)
    }
}

impl TryFrom<PathBuf> for BuildpackConfig {
    type Error = ConfigError;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        fs::read_to_string(&value)
            .map_err(|e| ConfigError::ReadConfig(value.clone(), e))
            .and_then(|contents| {
                BuildpackConfig::from_str(&contents).map_err(|e| ConfigError::ParseConfig(value, e))
            })
    }
}

impl FromStr for BuildpackConfig {
    type Err = ParseConfigError;

    fn from_str(contents: &str) -> Result<Self, Self::Err> {
        let doc = DocumentMut::from_str(contents).map_err(Self::Err::InvalidToml)?;

        // the root config is the table named `[com.heroku.buildpacks.deb-packages]` in project.toml
        let root_config_item = doc
            .get("com")
            .and_then(|item| item.as_table_like())
            .and_then(|com| com.get("heroku"))
            .and_then(|item| item.as_table_like())
            .and_then(|heroku| heroku.get("buildpacks"))
            .and_then(|item| item.as_table_like())
            .and_then(|buildpacks| buildpacks.get("deb-packages"));

        match root_config_item {
            None => Ok(BuildpackConfig::default()),
            Some(item) => item
                .as_table_like()
                .ok_or(Self::Err::WrongConfigType)
                .map(BuildpackConfig::try_from)?,
        }
    }
}

impl TryFrom<&dyn TableLike> for BuildpackConfig {
    type Error = ParseConfigError;

    fn try_from(config_item: &dyn TableLike) -> Result<Self, Self::Error> {
        let mut install = IndexSet::new();

        if let Some(install_values) = config_item.get("install").and_then(|item| item.as_array()) {
            for install_value in install_values {
                install.insert(
                    RequestedPackage::try_from(install_value)
                        .map_err(Self::Error::ParseRequestedPackage)?,
                );
            }
        }

        Ok(BuildpackConfig { install })
    }
}

#[derive(Debug)]
pub(crate) enum ConfigError {
    CheckExists(PathBuf, std::io::Error),
    ReadConfig(PathBuf, std::io::Error),
    ParseConfig(PathBuf, ParseConfigError),
}

#[derive(Debug)]
pub(crate) enum ParseConfigError {
    InvalidToml(toml_edit::TomlError),
    WrongConfigType,
    ParseRequestedPackage(ParseRequestedPackageError),
}

impl From<ConfigError> for DebianPackagesBuildpackError {
    fn from(value: ConfigError) -> Self {
        DebianPackagesBuildpackError::Config(value)
    }
}

impl From<ConfigError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: ConfigError) -> Self {
        Self::BuildpackError(value.into())
    }
}

#[cfg(test)]
mod test {
    use crate::debian::PackageName;

    use super::*;

    #[test]
    fn test_deserialize() {
        let toml = r#"
[_]
schema-version = "0.2"

[com.heroku.buildpacks.deb-packages]
install = [
    "package1",
    { name = "package2" },
    { name = "package3", skip_dependencies = true, force = true },
]
        "#
        .trim();
        let config = BuildpackConfig::from_str(toml).unwrap();
        assert_eq!(
            config,
            BuildpackConfig {
                install: IndexSet::from([
                    RequestedPackage {
                        name: PackageName::from_str("package1").unwrap(),
                        skip_dependencies: false,
                        force: false,
                    },
                    RequestedPackage {
                        name: PackageName::from_str("package2").unwrap(),
                        skip_dependencies: false,
                        force: false,
                    },
                    RequestedPackage {
                        name: PackageName::from_str("package3").unwrap(),
                        skip_dependencies: true,
                        force: true,
                    }
                ])
            }
        );
    }

    #[test]
    fn test_empty_root_config() {
        let toml = r#"
[_]
schema-version = "0.2"

[com.heroku.buildpacks.deb-packages]

        "#
        .trim();
        let config = BuildpackConfig::from_str(toml).unwrap();
        assert_eq!(config, BuildpackConfig::default());
    }

    #[test]
    fn test_missing_root_config() {
        let toml = r#"
[_]
schema-version = "0.2"
        "#
        .trim();
        let config = BuildpackConfig::from_str(toml).unwrap();
        assert_eq!(config, BuildpackConfig::default());
    }

    #[test]
    fn test_deserialize_with_invalid_package_name_as_string() {
        let toml = r#"
[_]
schema-version = "0.2"

[com.heroku.buildpacks.deb-packages]
install = [
    "not-a-package*",
]
        "#
        .trim();
        match BuildpackConfig::from_str(toml).unwrap_err() {
            ParseConfigError::ParseRequestedPackage(_) => {}
            e => panic!("Not the expected error - {e:?}"),
        }
    }

    #[test]
    fn test_deserialize_with_invalid_package_name_in_object() {
        let toml = r#"
[_]
schema-version = "0.2"

[com.heroku.buildpacks.deb-packages]
install = [
    { name = "not-a-package*" },
]
        "#
        .trim();
        match BuildpackConfig::from_str(toml).unwrap_err() {
            ParseConfigError::ParseRequestedPackage(_) => {}
            e => panic!("Not the expected error - {e:?}"),
        }
    }

    #[test]
    fn test_root_config_not_a_table() {
        let toml = r#"
[_]
schema-version = "0.2"

[com.heroku.buildpacks]
deb-packages = ["wrong"]

        "#
        .trim();
        match BuildpackConfig::from_str(toml).unwrap_err() {
            ParseConfigError::WrongConfigType => {}
            e => panic!("Not the expected error - {e:?}"),
        }
    }

    #[test]
    fn test_invalid_toml() {
        let toml = r"
![not valid toml
        "
        .trim();
        match BuildpackConfig::from_str(toml).unwrap_err() {
            ParseConfigError::InvalidToml(_) => {}
            e => panic!("Not the expected error - {e:?}"),
        }
    }
}
