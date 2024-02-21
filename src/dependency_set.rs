use std::collections::HashSet;
use std::ops::Deref;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

use fun_run::CommandWithName;

use crate::commands::apt::AptCacheCommand;
use crate::debian::{DebianPackageName, ParseDebianPackageNameError};

#[derive(Debug, PartialEq, Default)]
pub(crate) struct DependencySet(HashSet<DebianPackageName>);

impl DependencySet {
    pub(crate) fn from_apt_cache_depends(
        direct_dependencies: HashSet<DebianPackageName>,
        apt_config: PathBuf,
    ) -> Result<DependencySet, DependencySetError> {
        if direct_dependencies.is_empty() {
            return Ok(DependencySet::default());
        }

        let mut apt_cache = AptCacheCommand::new();
        apt_cache.config_file = Some(apt_config);
        apt_cache.important = true;
        apt_cache.recurse = true;

        let mut apt_cache_depends = apt_cache.depends();
        apt_cache_depends.packages = direct_dependencies;

        Command::from(apt_cache_depends)
            .named_output()
            .map_err(DependencySetError::AptCacheDependsCommand)
            .and_then(|output| DependencySet::from_str(&output.stdout_lossy()))
    }
}

impl Deref for DependencySet {
    type Target = HashSet<DebianPackageName>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> From<[DebianPackageName; N]> for DependencySet {
    fn from(value: [DebianPackageName; N]) -> Self {
        DependencySet(HashSet::from(value))
    }
}

impl FromStr for DependencySet {
    type Err = DependencySetError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .lines()
            .filter(is_debian_package_line)
            .map(DebianPackageName::from_str)
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|e| DependencySetError::ParseDebianPackageName(value.to_string(), e))
            .map(DependencySet)
    }
}

fn is_debian_package_line(line: &&str) -> bool {
    !is_empty_line(line) && !is_dependency_line(line) && !is_virtual_package(line)
}

fn is_empty_line(line: &str) -> bool {
    line.trim().is_empty()
}

// In the apt-cache depends output, indented lines are used to indicate the dependencies of a package
// that have the relationship (Depends or PreDepends) because we include the `--important` flag in the
// `apt-cache depends` command.
//
// We ignore these lines because the package names are duplicated without indentation in the output
// because we include the `--recurse` flag in the `apt-cache depends` command.
//
// See: https://www.debian.org/doc/debian-policy/ch-relationships.html#
fn is_dependency_line(line: &str) -> bool {
    line.starts_with(' ')
}

// Virtual packages seem to be output in the format `<package-name>`. These lines are ignored
// since they don't provide any version information when passed to the `apt-cache policy` command.
//
// See: https://www.debian.org/doc/debian-policy/ch-binary.html#virtual-packages
fn is_virtual_package(line: &str) -> bool {
    line.trim_start().starts_with('<') && line.trim_end().ends_with('>')
}

#[derive(Debug)]
#[allow(dead_code)] // TODO: remove after error handling is complete
pub(crate) enum DependencySetError {
    AptCacheDependsCommand(fun_run::CmdError),
    ParseDebianPackageName(String, ParseDebianPackageNameError),
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_dependency_set() {
        let value = "
xmlsec1
  Depends: libc6
  Depends: libxml2
  Depends: libxmlsec1
  Depends: libxmlsec1-openssl
  Depends: libxslt1.1
libssl3
  Depends: libc6
 |Depends: debconf
  Depends: <debconf-2.0>
    cdebconf
    debconf
gcc-12-base
debconf
  PreDepends: perl-base
<debconf-2.0>
        ";
        let dependency_set = value.parse::<DependencySet>().unwrap();
        assert_eq!(
            dependency_set,
            DependencySet::from([
                DebianPackageName::from_str("xmlsec1").unwrap(),
                DebianPackageName::from_str("libssl3").unwrap(),
                DebianPackageName::from_str("gcc-12-base").unwrap(),
                DebianPackageName::from_str("debconf").unwrap(),
            ])
        );
    }

    #[test]
    fn test_parse_dependency_set_with_bad_dependency_in_output() {
        let value = "
xmlsec1
  Depends: libc6
  Depends: libxml2
  Depends: libxmlsec1
  Depends: libxmlsec1-openssl
  Depends: libxslt1.1        
?bad-name!
        ";
        let error = value.parse::<DependencySet>().unwrap_err();
        match error {
            DependencySetError::ParseDebianPackageName(output, inner_error) => match inner_error {
                ParseDebianPackageNameError(bad_package_name) => {
                    assert_eq!(output, value);
                    assert_eq!(bad_package_name, "?bad-name!");
                }
            },
            DependencySetError::AptCacheDependsCommand(_) => panic!("Not the expected error"),
        }
    }

    #[test]
    fn test_empty_dependency_set_is_returned_when_given_an_empty_set_of_direct_dependencies() {
        assert_eq!(
            DependencySet::default(),
            DependencySet::from_apt_cache_depends(
                HashSet::default(),
                PathBuf::from("/not/a/real/path")
            )
            .unwrap()
        );
    }
}
