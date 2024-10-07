use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
#[allow(non_camel_case_types)]
// https://wiki.debian.org/Multiarch/Tuples
pub(crate) enum ArchitectureName {
    AMD_64,
    ARM_64,
}

impl FromStr for ArchitectureName {
    type Err = UnsupportedArchitectureNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "amd64" => Ok(ArchitectureName::AMD_64),
            "arm64" => Ok(ArchitectureName::ARM_64),
            _ => Err(UnsupportedArchitectureNameError(value.to_string())),
        }
    }
}

impl Display for ArchitectureName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchitectureName::AMD_64 => write!(f, "amd64"),
            ArchitectureName::ARM_64 => write!(f, "arm64"),
        }
    }
}

#[derive(Debug)]
// Due to how this error rolls into the broader `Distro::try_from(Target)` implementation, the
// architecture name stored in this struct isn't used directly which triggers a `dead_code`
// warning. I could eliminate this by not capturing the architecture name that failed to parse
// but that doesn't seem right. I'd rather keep this information attached to this struct even
// if it's not used at this point in time.
#[allow(dead_code)]
pub(crate) struct UnsupportedArchitectureNameError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_value_architecture_name() {
        assert_eq!(
            ArchitectureName::AMD_64,
            ArchitectureName::from_str("amd64").unwrap()
        );
    }

    #[test]
    fn parse_invalid_architecture_name() {
        match ArchitectureName::from_str("???").unwrap_err() {
            UnsupportedArchitectureNameError(value) => assert_eq!(value, "???"),
        }
    }

    #[test]
    fn display_architecture_name() {
        assert_eq!(ArchitectureName::AMD_64.to_string(), "amd64");
        assert_eq!(ArchitectureName::ARM_64.to_string(), "arm64");
    }
}
