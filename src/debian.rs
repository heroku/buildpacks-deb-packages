use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
// https://www.debian.org/doc/debian-policy/ch-controlfields.html#source
pub(crate) struct DebianPackageName(pub(crate) String);

impl AsRef<OsStr> for DebianPackageName {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
    }
}

impl FromStr for DebianPackageName {
    type Err = ParseDebianPackageNameError;

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
            Ok(DebianPackageName(value.to_string()))
        } else {
            Err(ParseDebianPackageNameError(value.to_string()))
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct ParseDebianPackageNameError(pub(crate) String);

#[derive(Debug, PartialEq)]
#[allow(non_camel_case_types)]
// https://wiki.debian.org/Multiarch/Tuples
pub(crate) enum DebianArchitectureName {
    AMD_64,
}

impl FromStr for DebianArchitectureName {
    type Err = ParseDebianArchitectureNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "amd64" => Ok(DebianArchitectureName::AMD_64),
            _ => Err(ParseDebianArchitectureNameError(value.to_string())),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ParseDebianArchitectureNameError(String);

#[derive(Debug, PartialEq)]
#[allow(non_camel_case_types)]
// https://wiki.debian.org/Multiarch/Tuples
pub(crate) enum DebianMultiarchName {
    X86_64_LINUX_GNU,
}

impl From<&DebianArchitectureName> for DebianMultiarchName {
    fn from(value: &DebianArchitectureName) -> Self {
        match value {
            DebianArchitectureName::AMD_64 => DebianMultiarchName::X86_64_LINUX_GNU,
        }
    }
}

impl Display for DebianMultiarchName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DebianMultiarchName::X86_64_LINUX_GNU => write!(f, "x86_64-linux-gnu"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_value_debian_architecture_name() {
        assert_eq!(
            DebianArchitectureName::AMD_64,
            DebianArchitectureName::from_str("amd64").unwrap()
        );
    }

    #[test]
    fn parse_invalid_debian_architecture_name() {
        match DebianArchitectureName::from_str("???").unwrap_err() {
            ParseDebianArchitectureNameError(value) => assert_eq!(value, "???"),
        }
    }

    #[test]
    fn converting_debian_architecture_name_to_multiarch_name() {
        assert_eq!(
            DebianMultiarchName::from(&DebianArchitectureName::AMD_64),
            DebianMultiarchName::X86_64_LINUX_GNU
        );
    }

    #[test]
    fn display_debian_to_multiarch_name() {
        assert_eq!(
            DebianMultiarchName::X86_64_LINUX_GNU.to_string(),
            "x86_64-linux-gnu"
        );
    }
}
