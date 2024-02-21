use crate::debian::{DebianPackageName, ParseDebianPackageNameError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Aptfile {
    pub(crate) packages: HashSet<DebianPackageName>,
}

impl FromStr for Aptfile {
    type Err = ParseAptfileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(DebianPackageName::from_str)
            .collect::<Result<HashSet<_>, _>>()
            .map_err(ParseAptfileError)
            .map(|packages| Aptfile { packages })
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct ParseAptfileError(ParseDebianPackageNameError);

impl Display for ParseAptfileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let parse_debian_package_name_error = &self.0;
        let invalid_debian_package_name = &parse_debian_package_name_error.0;
        write!(
            f,
            "Invalid debian package name: `{invalid_debian_package_name}`"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parse_aptfile() {
        let aptfile = Aptfile::from_str(indoc! { "
           # comment line
               # comment line with leading whitespace

            package-name-1
            package-name-2

            # Package name has leading and trailing whitespace
               package-name-3  \t
            # Duplicates are allowed (at least for now)
            package-name-1

        " })
        .unwrap();
        assert_eq!(
            aptfile,
            Aptfile {
                packages: HashSet::from([
                    DebianPackageName("package-name-1".to_string()),
                    DebianPackageName("package-name-2".to_string()),
                    DebianPackageName("package-name-3".to_string()),
                ])
            }
        );
    }

    #[test]
    fn parse_invalid_aptfile() {
        let error = Aptfile::from_str(indoc! { "
           invalid package name!
        " })
        .unwrap_err();
        assert_eq!(
            error,
            ParseAptfileError(ParseDebianPackageNameError(
                "invalid package name!".to_string()
            ))
        );
    }
}
