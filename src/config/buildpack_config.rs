use crate::config::custom_source::{CustomSource, ParseCustomSourceError};
use crate::config::{ParseRequestedPackageError, RequestedPackage};
use crate::DebianPackagesBuildpackError;
use indexmap::IndexSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml_edit::{DocumentMut, TableLike};

pub(crate) const NAMESPACED_CONFIG: &str = "com.heroku.buildpacks.deb-packages";

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct BuildpackConfig {
    pub(crate) install: IndexSet<RequestedPackage>,
    pub(crate) sources: Vec<CustomSource>,
}

impl BuildpackConfig {
    pub(crate) fn is_present(config_file: impl AsRef<Path>) -> Result<bool, ConfigError> {
        match BuildpackConfig::try_from(config_file.as_ref().to_path_buf()) {
            Ok(_) => Ok(true),
            Err(ConfigError::ParseConfig(_, ParseConfigError::MissingNamespacedConfig)) => {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }
}

impl TryFrom<PathBuf> for BuildpackConfig {
    type Error = ConfigError;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let contents = read_config_file(&value)?;
        BuildpackConfig::from_str(&contents).map_err(|e| ConfigError::ParseConfig(value, e))
    }
}

impl FromStr for BuildpackConfig {
    type Err = ParseConfigError;

    fn from_str(contents: &str) -> Result<Self, Self::Err> {
        let doc = parse_config_toml(contents)?;
        let config = get_buildpack_namespaced_config(&doc)?;
        BuildpackConfig::try_from(config)
    }
}

impl TryFrom<&dyn TableLike> for BuildpackConfig {
    type Error = ParseConfigError;

    fn try_from(config_item: &dyn TableLike) -> Result<Self, Self::Error> {
        let mut install = IndexSet::new();
        let mut sources = Vec::new();

        if let Some(install_values) = config_item.get("install").and_then(|item| item.as_array()) {
            for install_value in install_values {
                install.insert(
                    RequestedPackage::try_from(install_value)
                        .map_err(|e| Self::Error::ParseRequestedPackage(Box::new(e)))?,
                );
            }
        }

        if let Some(source_values) = config_item
            .get("sources")
            .and_then(|item| item.as_array_of_tables())
        {
            for source_value in source_values {
                sources.push(
                    CustomSource::try_from(source_value)
                        .map_err(|e| Self::Error::ParseCustomSource(Box::new(e)))?,
                );
            }
        }

        Ok(BuildpackConfig { install, sources })
    }
}

#[derive(Debug)]
pub(crate) enum ConfigError {
    ReadConfig(PathBuf, std::io::Error),
    ParseConfig(PathBuf, ParseConfigError),
}

#[derive(Debug)]
pub(crate) enum ParseConfigError {
    InvalidToml(toml_edit::TomlError),
    MissingNamespacedConfig,
    ParseRequestedPackage(Box<ParseRequestedPackageError>),
    ParseCustomSource(Box<ParseCustomSourceError>),
    WrongConfigType,
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

fn read_config_file(config_file: impl AsRef<Path>) -> Result<String, ConfigError> {
    let config_file = config_file.as_ref();
    fs::read_to_string(config_file)
        .map_err(|e| ConfigError::ReadConfig(config_file.to_path_buf(), e))
}

fn parse_config_toml(config: &str) -> Result<DocumentMut, ParseConfigError> {
    DocumentMut::from_str(config).map_err(ParseConfigError::InvalidToml)
}

fn get_buildpack_namespaced_config(doc: &DocumentMut) -> Result<&dyn TableLike, ParseConfigError> {
    let mut current_table = doc
        .as_item()
        .as_table_like()
        .ok_or(ParseConfigError::WrongConfigType)?;
    for name in NAMESPACED_CONFIG.split('.') {
        current_table = match current_table.get(name) {
            Some(item) => item
                .as_table_like()
                .ok_or(ParseConfigError::WrongConfigType),
            None => Err(ParseConfigError::MissingNamespacedConfig),
        }?;
    }
    Ok(current_table)
}

#[cfg(test)]
mod test {
    use crate::debian::ArchitectureName::{AMD_64, ARM_64};
    use crate::debian::PackageName;
    use indoc::indoc;

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
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

[[com.heroku.buildpacks.deb-packages.sources]]
uri = "http://archive.ubuntu.com/ubuntu"
suites = ["main"]
components = ["multiverse"]
arch = ["amd64", "arm64"]
signed_by = """-----BEGIN PGP PUBLIC KEY BLOCK-----

NxRt3Z+7w5HMIN2laKp+ItxloPWGBdcHU4o2ZnWgsVT8Y/a+RED75DDbAQ6lS3fV
sSlmQLExcf75qOPy34XNv3gWP4tbfIXXt8olflF8hwHggmKZzEImnzEozPabDsN7
nkhHZEWhGcPRcuHbFOqcirV1sfsKK1gOsTbxS00iD3OivOFCQqujF196cal/utTd
hVnssTC1arrx273zFepLosPvgrT0TS7tnyXbzuq5mo0zD1fSj4kuSS9V/SSy9fWF
LAtHiNQJkjzGFxu0/9dyQyX6C523uvfdcOzpObTyjBeGKqmEEf0lF5OYLDlkk2Sm
iGa6i2oLaGzGaQZDpdqyQZiYpQEYw9xN+8g=
=J31U
-----END PGP PUBLIC KEY BLOCK-----
"""
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
                ]),
                sources: Vec::from([CustomSource {
                    uri: "http://archive.ubuntu.com/ubuntu".into(),
                    suites: vec!["main".into()],
                    components: vec!["multiverse".into()],
                    arch: vec![AMD_64, ARM_64],
                    signed_by: indoc! { "
                        -----BEGIN PGP PUBLIC KEY BLOCK-----
                       
                        NxRt3Z+7w5HMIN2laKp+ItxloPWGBdcHU4o2ZnWgsVT8Y/a+RED75DDbAQ6lS3fV
                        sSlmQLExcf75qOPy34XNv3gWP4tbfIXXt8olflF8hwHggmKZzEImnzEozPabDsN7
                        nkhHZEWhGcPRcuHbFOqcirV1sfsKK1gOsTbxS00iD3OivOFCQqujF196cal/utTd
                        hVnssTC1arrx273zFepLosPvgrT0TS7tnyXbzuq5mo0zD1fSj4kuSS9V/SSy9fWF
                        LAtHiNQJkjzGFxu0/9dyQyX6C523uvfdcOzpObTyjBeGKqmEEf0lF5OYLDlkk2Sm
                        iGa6i2oLaGzGaQZDpdqyQZiYpQEYw9xN+8g=
                        =J31U
                        -----END PGP PUBLIC KEY BLOCK-----\n"
                    }
                    .into()
                }])
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
        match BuildpackConfig::from_str(toml).unwrap_err() {
            ParseConfigError::MissingNamespacedConfig => {}
            e => panic!("Not the expected error - {e:?}"),
        }
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
