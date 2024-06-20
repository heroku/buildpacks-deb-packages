use std::collections::{HashMap, HashSet};

use rayon::iter::{IntoParallelIterator, ParallelBridge, ParallelIterator};

use crate::debian::RepositoryUri;

#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) struct RepositoryPackage {
    pub(crate) repository_uri: RepositoryUri,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) filename: String,
    pub(crate) sha256sum: String,
    pub(crate) depends: Option<String>,
    pub(crate) pre_depends: Option<String>,
    pub(crate) provides: Option<String>,
}

impl RepositoryPackage {
    // NOTE: This is a simpler parser than what is provided by the `apt-parser` crate
    //       because we're indexing a large number of packages and the default
    //       parser was too slow.
    pub(crate) fn parse_parallel(
        repository_uri: RepositoryUri,
        contents: &str,
    ) -> Result<RepositoryPackage, ParseRepositoryPackageError> {
        let values = contents
            .lines()
            .par_bridge()
            .into_par_iter()
            .filter(|line| {
                [
                    PACKAGE_KEY,
                    VERSION_KEY,
                    FILENAME_KEY,
                    SHA256_KEY,
                    DEPENDS_KEY,
                    PRE_DEPENDS_KEY,
                    PROVIDES_KEY,
                ]
                .iter()
                .any(|key| line.starts_with(key))
            })
            .filter_map(|line| line.split_once(':'))
            .collect::<HashMap<&str, &str>>();

        Ok(RepositoryPackage {
            repository_uri,
            name: values
                .get(PACKAGE_KEY)
                .map(|v| v.trim().to_string())
                .ok_or(ParseRepositoryPackageError::MissingPackageName)?,
            version: values
                .get(VERSION_KEY)
                .map(|v| v.trim().to_string())
                .ok_or(ParseRepositoryPackageError::MissingVersionKey)?,
            filename: values
                .get(FILENAME_KEY)
                .map(|v| v.trim().to_string())
                .ok_or(ParseRepositoryPackageError::MissingFilename)?,
            sha256sum: values
                .get(SHA256_KEY)
                .map(|v| v.trim().to_string())
                .ok_or(ParseRepositoryPackageError::MissingSha256)?,
            depends: values.get(DEPENDS_KEY).map(|v| v.trim().to_string()),
            pre_depends: values.get(PRE_DEPENDS_KEY).map(|v| v.trim().to_string()),
            provides: values.get(PROVIDES_KEY).map(|v| v.trim().to_string()),
        })
    }

    // NOTE: This list deliberately ignores alternative dependencies specified by "|"
    //       as described by the debian package spec for relationship fields
    //       https://www.debian.org/doc/debian-policy/ch-relationships#syntax-of-relationship-fields
    //
    //       Until we want to support a more sophisticated dependency resolution process, this
    //       should suffice for constructing a simple dependency list. As such, we're only concerned
    //       here with packages names, not the version or architecture qualifiers that may be attached.
    pub(crate) fn get_dependencies(&self) -> HashSet<&str> {
        let mut results = HashSet::new();
        for field in [&self.pre_depends, &self.depends].into_iter().flatten() {
            // all dependencies are separated by commas
            for dependency in field.split(',') {
                // package name and optional version and/or architecture information is separated by whitespace
                if let Some(name) = dependency.trim().split(' ').next() {
                    // I couldn't find an explicit reference to why some packages have the
                    // format <package-name>:any (e.g.; python3:any) in the Debian Policy Manual
                    // but this seems limited to usage with virtual packages.
                    let name = match name.split(':').next() {
                        Some(virtual_package_name) => virtual_package_name.trim(),
                        None => name.trim(),
                    };
                    if !name.is_empty() {
                        results.insert(name);
                    }
                }
            }
        }
        results
    }

    pub(crate) fn provides_dependencies(&self) -> HashSet<&str> {
        let mut results = HashSet::new();
        if let Some(provides) = &self.provides {
            for provide in provides.split(',') {
                if let Some(name) = provide.trim().split(' ').next() {
                    let name = name.trim();
                    if !name.is_empty() {
                        results.insert(name);
                    }
                }
            }
        }
        results
    }
}

#[derive(Debug)]
pub(crate) enum ParseRepositoryPackageError {
    MissingPackageName,
    MissingVersionKey,
    MissingFilename,
    MissingSha256,
}

static PACKAGE_KEY: &str = "Package";
static VERSION_KEY: &str = "Version";
static FILENAME_KEY: &str = "Filename";
static SHA256_KEY: &str = "SHA256";
static DEPENDS_KEY: &str = "Depends";
static PRE_DEPENDS_KEY: &str = "Pre-Depends";
static PROVIDES_KEY: &str = "Provides";

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use crate::debian::{RepositoryPackage, RepositoryUri};

    fn create_repository_package(
        depends: Option<&str>,
        pre_depends: Option<&str>,
        provides: Option<&str>,
    ) -> RepositoryPackage {
        RepositoryPackage {
            repository_uri: RepositoryUri::from("test-repository".to_string()),
            name: "test-name".to_string(),
            version: "test-version".to_string(),
            filename: "test-filename".to_string(),
            sha256sum: "test-sha256sum".to_string(),
            depends: depends.map(ToString::to_string),
            pre_depends: pre_depends.map(ToString::to_string),
            provides: provides.map(ToString::to_string),
        }
    }

    #[test]
    fn test_empty_dependency_fields() {
        let repository_package = create_repository_package(None, None, None);
        assert_eq!(repository_package.get_dependencies(), HashSet::from([]));
    }

    #[test]
    fn test_depends_but_no_pre_depends_fields() {
        let repository_package = create_repository_package(Some("package1, package2"), None, None);
        assert_eq!(
            repository_package.get_dependencies(),
            HashSet::from(["package1", "package2"])
        );
    }

    #[test]
    fn test_pre_depends_but_no_depends_fields() {
        let repository_package = create_repository_package(None, Some("package1, package2"), None);
        assert_eq!(
            repository_package.get_dependencies(),
            HashSet::from(["package1", "package2"])
        );
    }

    #[test]
    fn test_pre_depends_and_depends_fields() {
        let repository_package =
            create_repository_package(Some("package1"), Some("package2"), None);
        assert_eq!(
            repository_package.get_dependencies(),
            HashSet::from(["package1", "package2"])
        );
    }

    #[test]
    fn test_package_dependency_variations() {
        let repository_package = create_repository_package(
            Some("package1 | optional-package"),
            Some("package2:any, package3 (>= 7:6.1), package4 (>= 2.34) [riscv64]"),
            None,
        );
        assert_eq!(
            repository_package.get_dependencies(),
            HashSet::from(["package1", "package2", "package3", "package4"])
        );
    }

    #[test]
    fn test_package_dependency_empty_strings() {
        let repository_package = create_repository_package(Some(""), Some(""), None);
        assert_eq!(repository_package.get_dependencies(), HashSet::from([]));
    }

    #[test]
    fn test_package_provides_variations() {
        let repository_package = create_repository_package(None, None, Some("bar (= 1.0), foo"));
        assert_eq!(
            repository_package.provides_dependencies(),
            HashSet::from(["bar", "foo"])
        );
    }

    #[test]
    fn test_package_provides_empty_string() {
        let repository_package = create_repository_package(None, None, Some(""));
        assert_eq!(
            repository_package.provides_dependencies(),
            HashSet::from([])
        );
    }
}
