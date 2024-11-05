use crate::debian::RepositoryPackage;
use indexmap::{IndexMap, IndexSet};
use std::str::FromStr;

#[derive(Debug, Default)]
pub(crate) struct PackageIndex {
    name_to_repository_packages: IndexMap<String, Vec<RepositoryPackage>>,
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
            .and_then(|repository_packages| {
                let mut sorted_repository_packages = Vec::with_capacity(repository_packages.len());
                for repository_package in repository_packages {
                    let parsed_version =
                        debversion::Version::from_str(repository_package.version.as_str())
                            .expect("Packages should always have a valid debian version");
                    sorted_repository_packages.push((repository_package, parsed_version));
                }

                sorted_repository_packages
                    .sort_by(|(_, version_a), (_, version_b)| version_b.cmp(version_a));

                sorted_repository_packages
                    .first()
                    .map(|(repository_package, _)| *repository_package)
            })
    }

    pub(crate) fn add_package(&mut self, package: RepositoryPackage) {
        for provides in package.provides_dependencies() {
            self.virtual_package_to_implementing_packages
                .entry(provides.to_string())
                .or_default()
                .push(package.clone());
        }

        self.name_to_repository_packages
            .entry(package.name.to_string())
            .or_default()
            .push(package);

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
            name: "test-name".to_string(),
            version: "test-version".to_string(),
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
            version: version.to_string(),
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
            version: version.to_string(),
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
