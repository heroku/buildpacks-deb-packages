use crate::aptfile::DebianPackage;
use std::collections::HashSet;
use std::ops::Deref;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

// https://manpages.ubuntu.com/manpages/jammy/en/man8/apt-get.8.html

#[derive(Debug, Default, Clone)]
pub(crate) struct AptGetCommand {
    pub(crate) allow_downgrades: bool,
    pub(crate) allow_remove_essential: bool,
    pub(crate) allow_change_held_packages: bool,
    pub(crate) assume_yes: bool,
    pub(crate) config_file: Option<PathBuf>,
    pub(crate) download_only: bool,
    pub(crate) force_yes: bool,
    pub(crate) reinstall: bool,
    pub(crate) version: bool,
}

impl AptGetCommand {
    pub(crate) fn new() -> Self {
        AptGetCommand::default()
    }

    pub(crate) fn install(&self) -> AptGetInstallCommand {
        AptGetInstallCommand::new(self.clone())
    }

    pub(crate) fn update(&self) -> AptGetUpdateCommand {
        AptGetUpdateCommand::new(self.clone())
    }
}

impl From<AptGetCommand> for Command {
    fn from(value: AptGetCommand) -> Self {
        let mut command = Command::new("apt-get");

        if value.allow_downgrades {
            command.arg("--allow-downgrades");
        }
        if value.allow_remove_essential {
            command.arg("--allow-remove-essential");
        }
        if value.allow_change_held_packages {
            command.arg("--allow-change-held-packages");
        }
        if value.assume_yes {
            command.arg("--assume-yes");
        }
        if let Some(config_file) = value.config_file {
            command.arg("--config-file");
            command.arg(config_file);
        }
        if value.download_only {
            command.arg("--download-only");
        }
        if value.force_yes {
            command.arg("--force-yes");
        }
        if value.reinstall {
            command.arg("--reinstall");
        }
        if value.version {
            command.arg("--version");
        }
        command
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AptGetInstallCommand {
    apt_get_command: AptGetCommand,
    pub(crate) packages: HashSet<DebianPackage>,
}

impl AptGetInstallCommand {
    fn new(apt_get_command: AptGetCommand) -> Self {
        Self {
            apt_get_command,
            packages: HashSet::new(),
        }
    }
}

impl From<AptGetInstallCommand> for Command {
    fn from(value: AptGetInstallCommand) -> Self {
        let mut command: Command = value.apt_get_command.into();
        command.arg("install");
        command.args(value.packages);
        command
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AptGetUpdateCommand {
    apt_get_command: AptGetCommand,
}

impl AptGetUpdateCommand {
    fn new(apt_get_command: AptGetCommand) -> Self {
        Self { apt_get_command }
    }
}

impl From<AptGetUpdateCommand> for Command {
    fn from(value: AptGetUpdateCommand) -> Self {
        let mut command: Command = value.apt_get_command.into();
        command.arg("update");
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
    fn test_apt_get_version() {
        let mut apt_get = AptGetCommand::new();
        apt_get.version = true;
        let command: Command = apt_get.into();
        assert_eq!(command.get_program(), "apt-get");
        assert_eq!(command.get_args().collect::<Vec<_>>(), &["--version"]);
    }

    #[test]
    fn test_apt_get_update() {
        let apt_get_update = AptGetCommand::new().update();
        let command: Command = apt_get_update.into();
        assert_eq!(command.get_program(), "apt-get");
        assert_eq!(command.get_args().collect::<Vec<_>>(), &["update"]);
    }

    #[test]
    fn test_apt_get_install_with_force_yes() {
        let mut apt_get = AptGetCommand::new();
        apt_get.assume_yes = true;
        apt_get.download_only = true;
        apt_get.reinstall = true;
        apt_get.force_yes = true;
        let mut apt_get_install = apt_get.install();
        apt_get_install
            .packages
            .insert(DebianPackage::from_str("some-package").unwrap());
        let command: Command = apt_get_install.into();
        assert_eq!(command.get_program(), "apt-get");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            &[
                "--assume-yes",
                "--download-only",
                "--force-yes",
                "--reinstall",
                "install",
                "some-package"
            ]
        );
    }

    #[test]
    fn test_apt_get_install_without_force_yes() {
        let mut apt_get = AptGetCommand::new();
        apt_get.assume_yes = true;
        apt_get.download_only = true;
        apt_get.reinstall = true;
        apt_get.allow_downgrades = true;
        apt_get.allow_remove_essential = true;
        apt_get.allow_change_held_packages = true;
        let mut apt_get_install = apt_get.install();
        apt_get_install
            .packages
            .insert(DebianPackage::from_str("some-package").unwrap());
        let command: Command = apt_get_install.into();
        assert_eq!(command.get_program(), "apt-get");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            &[
                "--allow-downgrades",
                "--allow-remove-essential",
                "--allow-change-held-packages",
                "--assume-yes",
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
