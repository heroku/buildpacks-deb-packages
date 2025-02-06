use std::str::FromStr;
use std::hash::{Hash, Hasher};

use toml_edit::{Formatted, InlineTable, Value};

use crate::debian::{PackageName, ParsePackageNameError};

#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) struct RequestedPackage {
    pub(crate) name: PackageName,
    pub(crate) skip_dependencies: bool,
    pub(crate) force: bool,
}

impl Hash for RequestedPackage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.skip_dependencies.hash(state);
        self.force.hash(state);
    }
}

impl FromStr for RequestedPackage {
    type Err = ParseRequestedPackageError;

    fn from_str(package_name: &str) -> Result<Self, Self::Err> {
        Ok(RequestedPackage {
            name: PackageName::from_str(package_name)
                .map_err(ParseRequestedPackageError::InvalidPackageName)?,
            skip_dependencies: false,
            force: false,
        })
    }
}

impl TryFrom<&Value> for RequestedPackage {
    type Error = ParseRequestedPackageError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(formatted_string) => RequestedPackage::try_from(formatted_string),
            Value::InlineTable(inline_table) => RequestedPackage::try_from(inline_table),
            _ => Err(ParseRequestedPackageError::UnexpectedTomlValue(
                value.clone(),
            )),
        }
    }
}

impl TryFrom<&Formatted<String>> for RequestedPackage {
    type Error = ParseRequestedPackageError;

    fn try_from(formatted_string: &Formatted<String>) -> Result<Self, Self::Error> {
        RequestedPackage::from_str(formatted_string.value())
    }
}

impl TryFrom<&InlineTable> for RequestedPackage {
    type Error = ParseRequestedPackageError;

    fn try_from(table: &InlineTable) -> Result<Self, Self::Error> {
        Ok(RequestedPackage {
            name: PackageName::from_str(
                table
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(ParseRequestedPackageError::InvalidPackageName)?,

            skip_dependencies: table
                .get("skip_dependencies")
                .and_then(Value::as_bool)
                .unwrap_or_default(),

            force: table
                .get("force")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug)]
pub(crate) enum ParseRequestedPackageError {
    InvalidPackageName(ParsePackageNameError),
    UnexpectedTomlValue(Value),
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::Array;

    #[test]
    fn test_from_str() {
        let package = RequestedPackage::from_str("package1").unwrap();
        assert_eq!(
            package,
            RequestedPackage {
                name: PackageName::from_str("package1").unwrap(),
                skip_dependencies: false,
                force: false,
            }
        );
    }

    #[test]
    fn test_try_from_with_env() {
        let mut table = InlineTable::new();
        table.insert("name", Value::from("package1"));
        let mut env_table = InlineTable::new();
        env_table.insert("ENV_VAR_1", Value::from("VALUE_1"));
        table.insert("env", Value::InlineTable(env_table));

        let package = RequestedPackage::try_from(&table).unwrap();
        assert_eq!(
            package,
            RequestedPackage {
                name: PackageName::from_str("package1").unwrap(),
                skip_dependencies: false,
                force: false,
            }
        );
    }

    #[test]
    fn test_try_from_with_commands() {
        let mut table = InlineTable::new();
        table.insert("name", Value::from("package1"));
        let mut commands_array = Array::new();
        commands_array.push("echo 'Hello, world!'");
        commands_array.push("ls -la");
        table.insert("commands", Value::Array(commands_array));

        let package = RequestedPackage::try_from(&table).unwrap();
        assert_eq!(
            package,
            RequestedPackage {
                name: PackageName::from_str("package1").unwrap(),
                skip_dependencies: false,
                force: false,
            }
        );
    }

    #[test]
    fn test_try_from_invalid_package_name() {
        let mut table = InlineTable::new();
        table.insert("name", Value::from("invalid/package/name"));

        let result = RequestedPackage::try_from(&table);
        assert!(result.is_err());
    }
}