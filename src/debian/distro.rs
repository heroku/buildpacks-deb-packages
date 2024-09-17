use std::str::FromStr;

use libcnb::Target;
use serde::{Deserialize, Serialize};

use crate::debian::ArchitectureName::{AMD_64, ARM_64};
use crate::debian::{ArchitectureName, DistroCodename, Source};
use crate::DebianPackagesBuildpackError;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct Distro {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) codename: DistroCodename,
    pub(crate) architecture: ArchitectureName,
}

impl Distro {
    pub(crate) fn get_source_list(&self) -> Vec<Source> {
        let source_list = match self.codename {
            DistroCodename::Jammy => get_jammy_source_list(),
            DistroCodename::Noble => get_noble_source_list(),
        };

        source_list
            .into_iter()
            .filter(|source| source.arch == self.architecture)
            .collect()
    }
}

impl TryFrom<&Target> for Distro {
    type Error = UnsupportedDistroError;

    fn try_from(target: &Target) -> Result<Self, Self::Error> {
        let name = target.distro_name.to_string();
        let version = target.distro_version.to_string();
        let target_arch = target.arch.to_string();

        let architecture =
            ArchitectureName::from_str(&target_arch).map_err(|_| UnsupportedDistroError {
                name: name.to_string(),
                version: version.to_string(),
                architecture: target_arch.to_string(),
            })?;

        match (name.to_lowercase().as_str(), version.as_str()) {
            ("ubuntu", "22.04") => Ok(Distro {
                name,
                version,
                architecture,
                codename: DistroCodename::Jammy,
            }),
            ("ubuntu", "24.04") => Ok(Distro {
                name,
                version,
                architecture,
                codename: DistroCodename::Noble,
            }),
            _ => Err(UnsupportedDistroError {
                name,
                version,
                architecture: target_arch,
            }),
        }
    }
}

// NOTE: Regarding http versus https for the repository urls that follow - these sources are extracted
//       from the default sources configured on these distributions which do not use https. This is
//       a trade-off between performance and privacy.
//
//       But, for security, we can verify no tampering of packages has occurred since release files
//       are validated with PGP and everything from that point on using checksums.
//
//       See: https://wiki.debian.org/SecureApt
//
//       The corresponding certificates used to validate the PGP signatures can be regenerated by
//       running <project-root>/scripts/extract_keys.sh.

fn get_jammy_source_list() -> Vec<Source> {
    vec![Source::new(
        // see note above for why http is used here instead of https
        "http://archive.ubuntu.com/ubuntu",
        vec!["jammy", "jammy-security", "jammy-updates"],
        vec!["main", "universe"],
        include_str!("../../keys/ubuntu_22.04.asc"),
        AMD_64,
    )]
}

fn get_noble_source_list() -> Vec<Source> {
    vec![
        Source::new(
            // see note above for why http is used here instead of https
            "http://archive.ubuntu.com/ubuntu",
            vec!["noble", "noble-updates"],
            vec!["main", "universe"],
            include_str!("../../keys/ubuntu_24.04.asc"),
            AMD_64,
        ),
        Source::new(
            // see note above for why http is used here instead of https
            "http://security.ubuntu.com/ubuntu",
            vec!["noble-security"],
            vec!["main", "universe"],
            include_str!("../../keys/ubuntu_24.04.asc"),
            AMD_64,
        ),
        Source::new(
            // see note above for why http is used here instead of https
            "http://ports.ubuntu.com/ubuntu-ports",
            vec!["noble", "noble-updates", "noble-security"],
            vec!["main", "universe"],
            include_str!("../../keys/ubuntu_24.04.asc"),
            ARM_64,
        ),
    ]
}

#[derive(Debug)]
pub(crate) struct UnsupportedDistroError {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) architecture: String,
}

impl From<UnsupportedDistroError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: UnsupportedDistroError) -> Self {
        Self::BuildpackError(DebianPackagesBuildpackError::UnsupportedDistro(value))
    }
}
