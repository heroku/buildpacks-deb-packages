use std::fmt::{Display, Formatter};
use std::str::FromStr;
use valuable::Valuable;

#[derive(Debug, Eq, PartialEq, Hash, Valuable)]
// https://www.debian.org/doc/debian-policy/ch-controlfields.html#source
pub(crate) struct PackageName(String);

impl PackageName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PackageName {
    type Err = ParsePackageNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // Package names (both source and binary, see Package) must consist only of
        // lower case letters (a-z), digits (0-9), plus (+) and minus (-) signs,
        // and periods (.). They must be at least two characters long and must
        // start with an alphanumeric character.
        let is_valid_package_name = value
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '+' | '-' | '.'))
            && value.chars().count() >= 2
            && value.starts_with(|c: char| c.is_ascii_alphanumeric());

        if is_valid_package_name {
            Ok(PackageName(value.to_string()))
        } else {
            Err(ParsePackageNameError {
                package_name: value.to_string(),
            })
        }
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct ParsePackageNameError {
    pub(crate) package_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_package_name() {
        let valid_names = [
            "a0",             // min length, starting with number
            "0a",             // min length, starting with letter
            "g++",            // alphanumeric to start followed by non-alphanumeric characters
            "libevent-2.1-6", // just a mix of allowed characters
            "a0+.-",          // all the allowed characters
        ];
        for valid_name in valid_names {
            assert_eq!(
                PackageName::from_str(valid_name).unwrap(),
                PackageName(valid_name.to_string())
            );
        }
    }

    #[test]
    fn parse_invalid_package_name() {
        let invalid_names = [
            "a",               // too short
            "+a",              // can't start with non-alphanumeric character
            "ab_c",            // can't contain invalid characters
            "aBc",             // uppercase is not allowed
            "package=1.2.3-1", // versioning is not allowed, package name only
        ];
        for invalid_name in invalid_names {
            assert_eq!(
                PackageName::from_str(invalid_name).unwrap_err(),
                ParsePackageNameError {
                    package_name: invalid_name.to_string()
                }
            );
        }
    }

    #[test]
    fn display_package_name() {
        assert_eq!(
            format!("{}", PackageName::from_str("my-package").unwrap()),
            "my-package"
        );
    }

    #[test]
    fn as_str() {
        assert_eq!(
            PackageName::from_str("my-package").unwrap().as_str(),
            "my-package"
        );
    }
}
