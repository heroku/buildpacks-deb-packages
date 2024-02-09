use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::ops::Deref;
use std::str::FromStr;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Aptfile {
    pub(crate) packages: HashSet<DebianPackage>,
}

#[derive(Debug)]
pub(crate) struct ParseAptfileError(ParseDebianPackageError);

impl FromStr for Aptfile {
    type Err = ParseAptfileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .lines()
            .filter_map(|mut line| {
                line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    None
                } else {
                    Some(line)
                }
            })
            .map(DebianPackage::from_str)
            .collect::<Result<HashSet<_>, _>>()
            .map_err(ParseAptfileError)
            .map(|packages| Aptfile { packages })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct DebianPackage(String);

impl Deref for DebianPackage {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<OsStr> for DebianPackage {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct ParseDebianPackageError(String);

impl FromStr for DebianPackage {
    type Err = ParseDebianPackageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            Err(ParseDebianPackageError(value.to_string()))
        } else {
            Ok(DebianPackage(value.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parse_valid_debian_package() {
        let debian_package = DebianPackage::from_str("package-name").unwrap();
        assert_eq!(*debian_package, "package-name".to_string());
    }
    #[test]
    fn parse_invalid_debian_package() {
        let error = DebianPackage::from_str("").unwrap_err();
        assert_eq!(error, ParseDebianPackageError("".to_string()));
    }

    #[test]
    fn parse_aptfile() {
        let aptfile = Aptfile::from_str(indoc! { "
            # comment line

            package-name-1
            package-name-2
            
        " })
        .unwrap();
        assert_eq!(
            aptfile.packages,
            HashSet::from([
                DebianPackage::from_str("package-name-1").unwrap(),
                DebianPackage::from_str("package-name-2").unwrap(),
            ])
        );
    }
}
