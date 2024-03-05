use crate::debian::DebianPackageName;
use std::collections::HashSet;
use std::ops::Deref;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

// https://manpages.ubuntu.com/manpages/jammy/en/man8/apt-get.8.html

#[derive(Debug, Clone)]
pub(crate) struct AptGetInstall {
    pub(crate) config_file: PathBuf,
    pub(crate) can_force_yes: bool,
    pub(crate) packages: HashSet<DebianPackageName>,
}

impl From<AptGetInstall> for Command {
    fn from(value: AptGetInstall) -> Self {
        let mut command = Command::new("apt-get");
        if value.can_force_yes {
            command.arg("--force-yes");
        } else {
            command.arg("--allow-downgrades");
            command.arg("--allow-remove-essential");
            command.arg("--allow-change-held-packages");
        }
        command.arg("--assume-yes");

        command.arg("--config-file");
        command.arg(value.config_file);

        command.arg("--download-only");
        command.arg("--reinstall");

        command.arg("install");
        command.args(value.packages);
        command
    }
}

#[derive(Debug)]
pub(crate) struct AptVersion(semver::Version);

#[derive(Debug)]
pub(crate) enum ParseAptVersionError {
    UnexpectedOutput(String),
    InvalidVersion(String, semver::Error),
}

impl FromStr for AptVersion {
    type Err = ParseAptVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let first_line = value
            .lines()
            .next()
            .ok_or(ParseAptVersionError::UnexpectedOutput(value.to_string()))?;

        let mut tokens = first_line.split(' ');
        if let Some("apt") = tokens.next() {
            tokens
                .next()
                .ok_or(ParseAptVersionError::UnexpectedOutput(value.to_string()))
                .and_then(|version_string| {
                    semver::Version::parse(version_string)
                        .map(AptVersion)
                        .map_err(|e| {
                            ParseAptVersionError::InvalidVersion(version_string.to_string(), e)
                        })
                })
        } else {
            Err(ParseAptVersionError::UnexpectedOutput(value.to_string()))
        }
    }
}

impl Deref for AptVersion {
    type Target = semver::Version;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_apt_get_install_with_force_yes() {
        let command: Command = AptGetInstall {
            packages: HashSet::from([DebianPackageName::from_str("some-package").unwrap()]),
            config_file: PathBuf::from("/dev/null"),
            can_force_yes: true,
        }
        .into();

        assert_eq!(command.get_program(), "apt-get");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            &[
                "--force-yes",
                "--assume-yes",
                "--config-file",
                "/dev/null",
                "--download-only",
                "--reinstall",
                "install",
                "some-package"
            ]
        );
    }

    #[test]
    fn test_apt_get_install_without_force_yes() {
        let command: Command = AptGetInstall {
            packages: HashSet::from([DebianPackageName::from_str("some-package").unwrap()]),
            config_file: PathBuf::from("/dev/null"),
            can_force_yes: false,
        }
        .into();

        assert_eq!(command.get_program(), "apt-get");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            &[
                "--allow-downgrades",
                "--allow-remove-essential",
                "--allow-change-held-packages",
                "--assume-yes",
                "--config-file",
                "/dev/null",
                "--download-only",
                "--reinstall",
                "install",
                "some-package"
            ]
        );
    }

    #[test]
    fn parse_apt_version() {
        let apt_version = AptVersion::from_str(indoc! { "
            apt 2.4.11 (amd64)
            Supported modules:
            *Ver: Standard .deb
             Pkg:  Debian APT solver interface (Priority -1000)
             Pkg:  Debian APT planner interface (Priority -1000)
            *Pkg:  Debian dpkg interface (Priority 30)
             S.L: 'deb' Debian binary tree
             S.L: 'deb-src' Debian source tree
             Idx: EDSP scenario file
             Idx: EIPP scenario file
             Idx: Debian Source Index
             Idx: Debian Package Index
             Idx: Debian Translation Index
             Idx: Debian dpkg status file
             Idx: Debian deb file
             Idx: Debian dsc file
             Idx: Debian control file
        " })
        .unwrap();
        assert_eq!(*apt_version, semver::Version::new(2, 4, 11));
    }

    #[test]
    fn parse_apt_version_with_unexpected_output() {
        let error = AptVersion::from_str("badoutput").unwrap_err();
        match error {
            ParseAptVersionError::UnexpectedOutput(output) => assert_eq!(&output, "badoutput"),
            ParseAptVersionError::InvalidVersion(_, _) => panic!("wrong error type"),
        };
    }

    #[test]
    fn parse_apt_version_with_invalid_semver() {
        let error = AptVersion::from_str("apt 1.2").unwrap_err();
        match error {
            ParseAptVersionError::UnexpectedOutput(_) => panic!("wrong error type"),
            ParseAptVersionError::InvalidVersion(version, _) => assert_eq!(&version, "1.2"),
        };
    }
}
