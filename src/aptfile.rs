use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::OnceLock;

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
        if debian_package_name_regex().is_match(value) {
            Ok(DebianPackage(value.to_string()))
        } else {
            Err(ParseDebianPackageError(value.to_string()))
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
    fn parse_valid_debian_package() {
        let debian_package = DebianPackage::from_str("package-name").unwrap();
        assert_eq!(*debian_package, "package-name".to_string());
    }
    #[test]
    fn parse_invalid_debian_package() {
        let invalid_names = [
            "a",    // too short
            "+a",   // can't start with non-alphanumeric character
            "ab_c", // can't contain invalid characters
            "aBc",  // uppercase is not allowed
        ];
        for invalid_name in invalid_names {
            assert_eq!(
                DebianPackage::from_str(invalid_name).unwrap_err(),
                ParseDebianPackageError(invalid_name.to_string())
            );
        }
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
