use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

use fun_run::CommandWithName;
use serde::{Deserialize, Serialize};

use crate::commands::apt::AptCacheCommand;
use crate::debian::{DebianPackageName, ParseDebianPackageNameError};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub(crate) struct DependencyVersions(HashMap<DebianPackageName, CandidateVersion>);

impl DependencyVersions {
    pub(crate) fn from_apt_cache_policy(
        packages: HashSet<DebianPackageName>,
        apt_config: PathBuf,
    ) -> Result<DependencyVersions, DependencyVersionsError> {
        if packages.is_empty() {
            return Ok(DependencyVersions::default());
        }

        let mut apt_cache = AptCacheCommand::new();
        apt_cache.config_file = Some(apt_config);

        let mut apt_cache_policy = apt_cache.policy();
        apt_cache_policy.packages = packages;

        Command::from(apt_cache_policy)
            .named_output()
            .map_err(DependencyVersionsError::AptCachePolicy)
            .and_then(|output| DependencyVersions::from_str(&output.stdout_lossy()))
    }
}

impl<const N: usize> From<[(DebianPackageName, CandidateVersion); N]> for DependencyVersions {
    fn from(value: [(DebianPackageName, CandidateVersion); N]) -> Self {
        DependencyVersions(HashMap::from(value))
    }
}

impl FromStr for DependencyVersions {
    type Err = DependencyVersionsError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut dependencies_versions = HashMap::new();
        let mut dependency_queue: Vec<DebianPackageName> = vec![];
        for line in value.lines().filter(|line| !line.is_empty()) {
            // dependency names are the only line that are not indented
            if line.starts_with(' ') {
                match (
                    line.trim().strip_prefix("Candidate: "),
                    &dependency_queue[..],
                ) {
                    // we have a version and a single package name in the queue, this is the expected format
                    (Some(version), [debian_package]) => {
                        let candidate_version = CandidateVersion(version.to_string());
                        dependencies_versions.insert(debian_package.clone(), candidate_version);
                        dependency_queue.pop();
                    }
                    // if there's zero or more than one package in the queue, that's a problem
                    (Some(_), [] | _) => {
                        Err(DependencyVersionsError::UnexpectedFormat(value.to_string()))?;
                    }
                    // ignore everything else, there's a check outside this loop to ensure nothing unprocessed is left in the queue
                    _ => continue,
                }
            } else if let Some(package_name) = line.strip_suffix(':') {
                let debian_package = DebianPackageName::from_str(package_name).map_err(|e| {
                    DependencyVersionsError::ParseDebianPackageName(value.to_string(), e)
                })?;
                dependency_queue.push(debian_package);
            }
        }
        // make sure we've processed all the captured package names
        if dependency_queue.is_empty() {
            Ok(DependencyVersions(dependencies_versions))
        } else {
            Err(DependencyVersionsError::UnexpectedFormat(value.to_string()))
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)] // TODO: remove once error handling is added
pub(crate) enum DependencyVersionsError {
    AptCachePolicy(fun_run::CmdError),
    ParseDebianPackageName(String, ParseDebianPackageNameError),
    UnexpectedFormat(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct CandidateVersion(pub(crate) String);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_dependency_versions() {
        let value = "
xmlsec1:
  Installed: (none)
  Candidate: 1.2.33-1build2
  Version table:
     1.2.33-1build2 500
        500 http://archive.ubuntu.com/ubuntu jammy/main amd64 Packages
libxmlsec1:
  Installed: (none)
  Candidate: 1.2.33-1build2
  Version table:
     1.2.33-1build2 500
        500 http://archive.ubuntu.com/ubuntu jammy/main amd64 Packages
        ";

        let dependency_versions = value.parse::<DependencyVersions>().unwrap();
        assert_eq!(
            dependency_versions,
            DependencyVersions::from([
                (
                    DebianPackageName("xmlsec1".to_string()),
                    CandidateVersion("1.2.33-1build2".to_string())
                ),
                (
                    DebianPackageName("libxmlsec1".to_string()),
                    CandidateVersion("1.2.33-1build2".to_string())
                )
            ])
        );
    }

    #[test]
    fn test_parse_empty_dependency_versions() {
        let value = "";
        let dependency_versions = value.parse::<DependencyVersions>().unwrap();
        assert_eq!(dependency_versions, DependencyVersions(HashMap::new()));
    }

    #[test]
    fn test_parse_dependency_versions_when_candidate_version_is_missing_before_next_dependency_in_list(
    ) {
        let value = "
xmlsec1:
  Installed: (none)
libxmlsec1:
  Installed: (none)
  Candidate: 1.2.33-1build2
  Version table:
     1.2.33-1build2 500
        500 http://archive.ubuntu.com/ubuntu jammy/main amd64 Packages
        ";
        let error = value.parse::<DependencyVersions>().unwrap_err();
        match error {
            DependencyVersionsError::UnexpectedFormat(input) => assert_eq!(value, input),
            _ => panic!("Wrong error"),
        };
    }

    #[test]
    fn test_parse_dependency_versions_when_candidate_version_is_found_before_any_dependency_in_list(
    ) {
        let value = "
  Installed: (none)
  Candidate: 1.2.33-1build2
libxmlsec1:
  Installed: (none)
  Candidate: 1.2.33-1build2
  Version table:
     1.2.33-1build2 500
        500 http://archive.ubuntu.com/ubuntu jammy/main amd64 Packages
        ";
        let error = value.parse::<DependencyVersions>().unwrap_err();
        match error {
            DependencyVersionsError::UnexpectedFormat(input) => assert_eq!(value, input),
            _ => panic!("Wrong error"),
        };
    }

    #[test]
    fn test_parse_dependency_versions_when_candidate_version_is_missing_when_dependency_is_last() {
        let value = "
xmlsec1:
  Installed: (none)
        ";
        let error = value.parse::<DependencyVersions>().unwrap_err();
        match error {
            DependencyVersionsError::UnexpectedFormat(input) => assert_eq!(input, value),
            _ => panic!("Wrong error"),
        };
    }

    #[test]
    fn test_parse_dependency_versions_when_dependency_name_is_invalid() {
        let value = "
?bad-name!:
  Installed: (none)
  Candidate: 1.2.33-1build2
  Version table:
     1.2.33-1build2 500
        500 http://archive.ubuntu.com/ubuntu jammy/main amd64 Packages
        ";
        let error = value.parse::<DependencyVersions>().unwrap_err();
        match error {
            DependencyVersionsError::ParseDebianPackageName(output, inner_error) => {
                match inner_error {
                    ParseDebianPackageNameError(name) => {
                        assert_eq!(output, value);
                        assert_eq!(name, "?bad-name!");
                    }
                }
            }
            _ => panic!("Wrong error"),
        };
    }

    #[test]
    fn test_empty_dependency_versions_is_returned_when_given_an_empty_set_of_dependencies() {
        assert_eq!(
            DependencyVersions::default(),
            DependencyVersions::from_apt_cache_policy(
                HashSet::default(),
                PathBuf::from("/not/a/real/path")
            )
            .unwrap()
        );
    }
}
