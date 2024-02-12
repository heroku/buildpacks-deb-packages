use std::path::PathBuf;
use std::process::Command;

// https://manpages.ubuntu.com/manpages/jammy/en/man1/dpkg-deb.1.html
// https://manpages.ubuntu.com/manpages/jammy/en/man1/dpkg-deb.1.html
#[derive(Debug, Default, Clone)]
pub(crate) struct DpkgCommand {
    extract_action: Option<(PathBuf, PathBuf)>,
}

impl DpkgCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn extract(&mut self, archive: PathBuf, target_directory: PathBuf) {
        self.extract_action = Some((archive, target_directory));
    }
}

impl From<DpkgCommand> for Command {
    fn from(value: DpkgCommand) -> Self {
        let mut command = Command::new("dpkg");
        if let Some((archive, target_directory)) = value.extract_action {
            command.arg("--extract");
            command.arg(archive);
            command.arg(target_directory);
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpkg_extract() {
        let mut dpkg = DpkgCommand::new();
        dpkg.extract(
            PathBuf::from("some-archive.deb"),
            PathBuf::from("target-directory"),
        );
        let command: Command = dpkg.into();
        assert_eq!(command.get_program(), "dpkg");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            &["--extract", "some-archive.deb", "target-directory"]
        );
    }
}
