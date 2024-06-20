use std::str::FromStr;

use libcnb::Target;

use crate::debian::ArchitectureName::{AMD_64, ARM_64};
use crate::debian::{ArchitectureName, DistroCodename, Source};

#[derive(Debug, Clone)]
pub(crate) struct SupportedDistro {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) codename: DistroCodename,
    pub(crate) architecture: ArchitectureName,
}

impl SupportedDistro {
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

impl TryFrom<&Target> for SupportedDistro {
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

        match (name.as_str(), version.as_str()) {
            ("ubuntu", "22.04") => Ok(SupportedDistro {
                name,
                version,
                architecture,
                codename: DistroCodename::Jammy,
            }),
            ("ubuntu", "24.04") => Ok(SupportedDistro {
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

fn get_jammy_source_list() -> Vec<Source> {
    vec![Source::new(
        "http://archive.ubuntu.com/ubuntu",
        vec!["jammy", "jammy-security", "jammy-updates"],
        vec!["main", "universe"],
        include_str!("../../keys/ubuntu-keyring-2018-archive.asc"),
        AMD_64,
    )]
}

fn get_noble_source_list() -> Vec<Source> {
    vec![
        Source::new(
            "http://archive.ubuntu.com/ubuntu",
            vec!["noble", "noble-updates"],
            vec!["main", "universe"],
            include_str!("../../keys/ubuntu-keyring-2018-archive.asc"),
            AMD_64,
        ),
        Source::new(
            "http://security.ubuntu.com/ubuntu",
            vec!["noble-security"],
            vec!["main", "universe"],
            include_str!("../../keys/ubuntu-keyring-2018-archive.asc"),
            AMD_64,
        ),
        Source::new(
            "http://ports.ubuntu.com/ubuntu-ports",
            vec!["noble", "noble-updates", "noble-security"],
            vec!["main", "universe"],
            include_str!("../../keys/ubuntu-keyring-2018-archive.asc"),
            ARM_64,
        ),
    ]
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct UnsupportedDistroError {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) architecture: String,
}
