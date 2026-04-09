use crate::debian::{RepositoryPackage, SourceOrder};
use indexmap::{IndexMap, IndexSet};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageResolutionKey {
    version: debversion::Version,
    source_order: SourceOrder,
}

impl PackageResolutionKey {
    fn new(version: debversion::Version, source_order: SourceOrder) -> Self {
        Self {
            version,
            source_order,
        }
    }
}

impl Ord for PackageResolutionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher version first, then lower source order first (first-declared wins)
        other
            .version
            .cmp(&self.version)
            .then(self.source_order.cmp(&other.source_order))
    }
}

impl PartialOrd for PackageResolutionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
pub(crate) struct PackageIndex {
    name_to_repository_packages:
        IndexMap<String, BTreeMap<PackageResolutionKey, RepositoryPackage>>,
    // NOTE: virtual packages are declared in the `Provides` field of a package
    //       https://www.debian.org/doc/debian-policy/ch-relationships.html#virtual-packages-provides
    virtual_package_to_implementing_packages: IndexMap<String, Vec<RepositoryPackage>>,
    pub(crate) packages_indexed: usize,
}

impl PackageIndex {
    pub(crate) fn get_highest_available_version(
        &self,
        package_name: &str,
    ) -> Option<&RepositoryPackage> {
        self.name_to_repository_packages
            .get(package_name)
            .and_then(|entries| entries.first_key_value())
            .map(|(_, pkg)| pkg)
    }

    pub(crate) fn add_package(&mut self, package: RepositoryPackage) {
        for provides in package.provides_dependencies() {
            self.virtual_package_to_implementing_packages
                .entry(provides.to_string())
                .or_default()
                .push(package.clone());
        }

        // NOTE: If a duplicate (same version + source order) is inserted, it silently
        // overwrites the previous entry. This shouldn't occur in practice since a given
        // source/suite/component can't produce two entries with the same package name and version.
        let key = PackageResolutionKey::new(package.version.clone(), package.source_order);
        self.name_to_repository_packages
            .entry(package.name.clone())
            .or_default()
            .insert(key, package);

        self.packages_indexed += 1;
    }

    pub(crate) fn get_providers(&self, package: &str) -> IndexSet<&str> {
        self.virtual_package_to_implementing_packages
            .get(package)
            .map(|provides| {
                provides
                    .iter()
                    .map(|provide| provide.name.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn get_package_names(&self) -> IndexSet<&str> {
        let mut package_names = self
            .name_to_repository_packages
            .keys()
            .map(String::as_str)
            .collect::<IndexSet<_>>();
        let virtual_package_names = self
            .virtual_package_to_implementing_packages
            .keys()
            .map(String::as_str)
            .collect::<IndexSet<_>>();
        package_names.extend(virtual_package_names.iter());
        package_names
    }
}

#[cfg(test)]
mod test {
    use crate::debian::RepositoryUri;

    use super::*;

    fn default_test_repository_package() -> RepositoryPackage {
        RepositoryPackage {
            repository_uri: RepositoryUri::from("test-repository"),
            source_order: SourceOrder::new(0, 0, 0),
            name: "test-name".to_string(),
            version: "1.0.0".parse().unwrap(),
            filename: "test-filename".to_string(),
            sha256sum: "test-sha256sum".to_string(),
            depends: None,
            pre_depends: None,
            provides: None,
        }
    }

    fn create_repository_package(name: &str, version: &str) -> RepositoryPackage {
        RepositoryPackage {
            name: name.to_string(),
            version: version.parse().unwrap(),
            ..default_test_repository_package()
        }
    }

    fn create_repository_package_with_source_order(
        name: &str,
        version: &str,
        repository_uri: &str,
        source_order: SourceOrder,
    ) -> RepositoryPackage {
        RepositoryPackage {
            name: name.to_string(),
            version: version.parse().unwrap(),
            repository_uri: RepositoryUri::from(repository_uri),
            source_order,
            ..default_test_repository_package()
        }
    }

    fn create_repository_package_with_provides(
        name: &str,
        version: &str,
        provides: &str,
    ) -> RepositoryPackage {
        RepositoryPackage {
            name: name.to_string(),
            version: version.parse().unwrap(),
            provides: Some(provides.to_string()),
            ..default_test_repository_package()
        }
    }

    #[test]
    fn test_missing_package() {
        let package_index = PackageIndex::default();
        assert_eq!(
            package_index.get_highest_available_version("my-package"),
            None
        );
    }

    #[test]
    fn test_adding_and_retrieving_single_package() {
        let mut package_index = PackageIndex::default();
        package_index.add_package(create_repository_package("my-package", "1.0.0"));
        assert_eq!(
            package_index.get_highest_available_version("my-package"),
            Some(&create_repository_package("my-package", "1.0.0"))
        );
    }

    #[test]
    fn test_retrieving_highest_available_package_version() {
        let mut package_index = PackageIndex::default();
        package_index.add_package(create_repository_package("my-package", "1.0.0"));
        package_index.add_package(create_repository_package("my-package", "2.0.0"));
        package_index.add_package(create_repository_package("my-package", "1.5.0"));
        assert_eq!(
            package_index.get_highest_available_version("my-package"),
            Some(&create_repository_package("my-package", "2.0.0"))
        );
    }

    #[test]
    fn test_same_version_different_priorities_prefers_lower_priority() {
        let mut package_index = PackageIndex::default();
        package_index.add_package(create_repository_package_with_source_order(
            "curl",
            "8.5.0-2ubuntu10.8",
            "http://security.ubuntu.com/ubuntu",
            SourceOrder::new(1, 0, 0),
        ));
        package_index.add_package(create_repository_package_with_source_order(
            "curl",
            "8.5.0-2ubuntu10.8",
            "http://archive.ubuntu.com/ubuntu",
            SourceOrder::new(0, 0, 0),
        ));
        let resolved = package_index
            .get_highest_available_version("curl")
            .expect("package should exist");
        assert_eq!(
            resolved.repository_uri,
            RepositoryUri::from("http://archive.ubuntu.com/ubuntu"),
            "When the same version exists in multiple sources, the first-declared source should win"
        );
    }

    #[test]
    fn test_higher_version_wins_regardless_of_priority() {
        let mut package_index = PackageIndex::default();
        package_index.add_package(create_repository_package_with_source_order(
            "curl",
            "7.0.0",
            "http://archive.ubuntu.com/ubuntu",
            SourceOrder::new(0, 0, 0),
        ));
        package_index.add_package(create_repository_package_with_source_order(
            "curl",
            "8.5.0",
            "http://security.ubuntu.com/ubuntu",
            SourceOrder::new(1, 0, 0),
        ));
        let resolved = package_index
            .get_highest_available_version("curl")
            .expect("package should exist");
        assert_eq!(
            resolved.repository_uri,
            RepositoryUri::from("http://security.ubuntu.com/ubuntu"),
            "Higher version should win even from a lower-priority source"
        );
    }

    #[test]
    fn test_duplicate_version_and_source_order_last_insert_wins() {
        let mut package_index = PackageIndex::default();
        package_index.add_package(create_repository_package_with_source_order(
            "curl",
            "8.5.0",
            "http://archive.ubuntu.com/ubuntu",
            SourceOrder::new(0, 0, 0),
        ));
        package_index.add_package(create_repository_package_with_source_order(
            "curl",
            "8.5.0",
            "http://mirror.ubuntu.com/ubuntu",
            SourceOrder::new(0, 0, 0),
        ));
        let resolved = package_index
            .get_highest_available_version("curl")
            .expect("package should exist");
        assert_eq!(
            resolved.repository_uri,
            RepositoryUri::from("http://mirror.ubuntu.com/ubuntu"),
            "When version and source order are identical, the last-inserted entry overwrites"
        );
    }

    #[test]
    fn test_get_virtual_package_providers() {
        let mut package_index = PackageIndex::default();
        let libvips_provider_1 =
            create_repository_package_with_provides("libvips42", "8.12.1-1build1", "libvips");
        let libvips_provider_2 =
            create_repository_package_with_provides("another-libvips-provider", "1.0.0", "libvips");
        package_index.add_package(libvips_provider_1.clone());
        package_index.add_package(libvips_provider_2.clone());
        assert_eq!(
            package_index.get_providers("libvips"),
            IndexSet::from([
                libvips_provider_1.name.as_str(),
                libvips_provider_2.name.as_str()
            ])
        );
    }

    #[test]
    fn test_get_virtual_package_providers_with_non_virtual_package() {
        let mut package_index = PackageIndex::default();
        let libvips_provider_1 =
            create_repository_package_with_provides("libvips42", "8.12.1-1build1", "libvips");
        package_index.add_package(libvips_provider_1);
        assert!(package_index.get_providers("libvips42").is_empty());
    }
}
