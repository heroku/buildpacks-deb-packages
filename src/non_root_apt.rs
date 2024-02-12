use crate::commands::apt_get::{AptGetCommand, AptVersion, ParseAptVersionError};
use fun_run::{CmdError, CommandWithName};
use indoc::formatdoc;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{tempdir, TempDir};

pub(crate) struct NonRootApt {
    pub(crate) apt_version: AptVersion,
    install_dir: TempDir,
}

#[derive(Debug)]
pub(crate) enum NonRootAptError {
    ConfigureApt(std::io::Error),
    VersionCommand(CmdError),
    VersionParse(ParseAptVersionError),
}

impl NonRootApt {
    pub(crate) fn new() -> Result<Self, NonRootAptError> {
        let install_dir = tempdir().map_err(NonRootAptError::ConfigureApt)?;

        // apt-get complains if these folders aren't present
        fs::create_dir_all(install_dir.path().join("state/lists/partial"))
            .map_err(NonRootAptError::ConfigureApt)?;
        fs::create_dir_all(install_dir.path().join("cache/archives/partial"))
            .map_err(NonRootAptError::ConfigureApt)?;

        // https://manpages.ubuntu.com/manpages/jammy/man5/apt.conf.5.html
        // set a custom apt.conf so that our calls to apt-get use our custom installation
        fs::write(
            install_dir.path().join("apt.conf"),
            formatdoc! { r#"
                #clear APT::Update::Post-Invoke;
                Debug::NoLocking "true";
                Dir::Cache "{dir}/cache";
                Dir::State "{dir}/state";      
            "#, dir = install_dir.path().to_string_lossy() },
        )
        .map_err(NonRootAptError::ConfigureApt)?;

        let mut apt_get = AptGetCommand::new();
        apt_get.version = true;

        let apt_version = Command::from(apt_get)
            .named_output()
            .map_err(NonRootAptError::VersionCommand)
            .and_then(|output| {
                output
                    .stdout_lossy()
                    .parse::<AptVersion>()
                    .map_err(NonRootAptError::VersionParse)
            })?;

        Ok(Self {
            install_dir,
            apt_version,
        })
    }

    pub(crate) fn apt_get(&self) -> AptGetCommand {
        let mut apt_get = AptGetCommand::new();
        apt_get.config_file = Some(self.install_dir.path().join("apt.conf"));
        apt_get
    }

    pub(crate) fn list_downloaded_debian_packages(&self) -> Result<Vec<PathBuf>, std::io::Error> {
        let mut archives = vec![];
        for result in fs::read_dir(self.install_dir.path().join("cache/archives"))? {
            let entry = result?;
            if entry.file_name().to_string_lossy().ends_with(".deb") {
                archives.push(entry.path());
            }
        }
        Ok(archives)
    }
}
