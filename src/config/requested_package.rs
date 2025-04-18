use crate::debian::{PackageName, ParsePackageNameError};
use serde::Serialize;
use std::str::FromStr;
use toml_edit::{Formatted, InlineTable, Value};

#[derive(Debug, Eq, PartialEq, Hash, Serialize)]
pub(crate) struct RequestedPackage {
    pub(crate) name: PackageName,
    pub(crate) skip_dependencies: bool,
    pub(crate) force: bool,
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
