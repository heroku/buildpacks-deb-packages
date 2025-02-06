use std::fmt::{Display, Formatter};

use crate::debian::ArchitectureName;
use std::str::FromStr;

#[derive(Debug, PartialEq, Clone)]
#[allow(non_camel_case_types)]
// https://wiki.debian.org/Multiarch/Tuples
pub(crate) enum MultiarchName {
    X86_64_LINUX_GNU,
    AARCH_64_LINUX_GNU,
}

impl From<&ArchitectureName> for MultiarchName {
    fn from(value: &ArchitectureName) -> Self {
        match value {
            ArchitectureName::AMD_64 => MultiarchName::X86_64_LINUX_GNU,
            ArchitectureName::ARM_64 => MultiarchName::AARCH_64_LINUX_GNU,
        }
    }
}

impl Display for MultiarchName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiarchName::X86_64_LINUX_GNU => write!(f, "x86_64-linux-gnu"),
            MultiarchName::AARCH_64_LINUX_GNU => write!(f, "aarch64-linux-gnu"),
        }
    }
}

impl FromStr for MultiarchName {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x86_64-linux-gnu" => Ok(MultiarchName::X86_64_LINUX_GNU),
            "aarch64-linux-gnu" => Ok(MultiarchName::AARCH_64_LINUX_GNU),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_multiarch_name_from_str() {
        // Test valid strings
        assert_eq!(MultiarchName::from_str("x86_64-linux-gnu").unwrap(), MultiarchName::X86_64_LINUX_GNU);
        assert_eq!(MultiarchName::from_str("aarch64-linux-gnu").unwrap(), MultiarchName::AARCH_64_LINUX_GNU);

        // Test invalid string
        assert!(MultiarchName::from_str("invalid-arch").is_err());
    }

    #[test]
    fn converting_architecture_name_to_multiarch_name() {
        assert_eq!(
            MultiarchName::from(&ArchitectureName::AMD_64),
            MultiarchName::X86_64_LINUX_GNU
        );
        assert_eq!(
            MultiarchName::from(&ArchitectureName::ARM_64),
            MultiarchName::AARCH_64_LINUX_GNU
        );
    }

    #[test]
    fn display_to_multiarch_name() {
        assert_eq!(
            MultiarchName::X86_64_LINUX_GNU.to_string(),
            "x86_64-linux-gnu"
        );
        assert_eq!(
            MultiarchName::AARCH_64_LINUX_GNU.to_string(),
            "aarch64-linux-gnu"
        );
    }
}
