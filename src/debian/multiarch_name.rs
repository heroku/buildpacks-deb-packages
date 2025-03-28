use crate::debian::ArchitectureName;
use std::fmt::{Display, Formatter};

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

#[cfg(test)]
mod tests {
    use super::*;

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
