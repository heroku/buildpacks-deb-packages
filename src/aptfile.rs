use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::OnceLock;

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub(crate) struct Aptfile {
    packages: HashSet<DebianPackageName>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ParseAptfileError(ParseDebianPackageNameError);

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

#[derive(Debug, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
pub(crate) struct DebianPackageName(String);

#[derive(Debug, PartialEq)]
pub(crate) struct ParseDebianPackageNameError(String);

impl FromStr for DebianPackageName {
    type Err = ParseDebianPackageNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if debian_package_name_regex().is_match(value) {
            Ok(DebianPackageName(value.to_string()))
        } else {
            Err(ParseDebianPackageNameError(value.to_string()))
        }
    }
}

fn debian_package_name_regex() -> &'static regex_lite::Regex {
    static LAZY: OnceLock<regex_lite::Regex> = OnceLock::new();
    LAZY.get_or_init(|| {
        // https://www.debian.org/doc/debian-policy/ch-controlfields.html#source
        // Package names (both source and binary, see Package) must consist only of
        // lower case letters (a-z), digits (0-9), plus (+) and minus (-) signs,
        // and periods (.). They must be at least two characters long and must
        // start with an alphanumeric character.
        regex_lite::Regex::new("^[a-z0-9][a-z0-9+.\\-]+$").expect("should be a valid regex pattern")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parse_valid_debian_package_name() {
        let valid_names = [
            "a0",             // min length, starting with number
            "0a",             // min length, starting with letter
            "g++",            // alphanumeric to start followed by non-alphanumeric characters
            "libevent-2.1-6", // just a mix of allowed characters
            "a0+.-",          // all the allowed characters
        ];
        for valid_name in valid_names {
            assert_eq!(
                DebianPackageName::from_str(valid_name).unwrap(),
                DebianPackageName(valid_name.to_string())
            );
        }
    }
    #[test]
    fn parse_invalid_debian_package_name() {
        let invalid_names = [
            "a",               // too short
            "+a",              // can't start with non-alphanumeric character
            "ab_c",            // can't contain invalid characters
            "aBc",             // uppercase is not allowed
            "package=1.2.3-1", // versioning is not allowed, package name only
        ];
        for invalid_name in invalid_names {
            assert_eq!(
                DebianPackageName::from_str(invalid_name).unwrap_err(),
                ParseDebianPackageNameError(invalid_name.to_string())
            );
        }
    }

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
            aptfile.packages,
            HashSet::from([
                DebianPackageName("package-name-1".to_string()),
                DebianPackageName("package-name-2".to_string()),
                DebianPackageName("package-name-3".to_string()),
            ])
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
