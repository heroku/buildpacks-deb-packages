use apt_parser::Control;
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};
use std::fs::read_to_string;
use std::path::PathBuf;

use crate::config::RequestedPackage;
use crate::debian::{PackageIndex, RepositoryPackage};
use crate::{BuildpackResult, DebianPackagesBuildpackError};

pub(crate) fn determine_packages_to_install(
    package_index: &PackageIndex,
    requested_packages: IndexSet<RequestedPackage>,
) -> BuildpackResult<Vec<RepositoryPackage>> {
    println!("## Determining packages to install");
    println!();

    let system_packages_path = PathBuf::from("/var/lib/dpkg/status");
    let system_packages = read_to_string(&system_packages_path)
        .map_err(|e| {
            DeterminePackagesToInstallError::ReadSystemPackages(system_packages_path.clone(), e)
        })?
        .trim()
        .split("\n\n")
        .map(|control_data| {
            Control::from(control_data)
                .map_err(|e| {
                    DeterminePackagesToInstallError::ParseSystemPackage(
                        system_packages_path.clone(),
                        control_data.to_string(),
                        e,
                    )
                })
                .map(|control| (control.package.to_string(), control))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    let mut install_details = IndexMap::new();
    for requested_package in requested_packages {
        let mut visit_stack = IndexSet::new();
        visit(
            requested_package.name.as_str(),
            requested_package.skip_dependencies,
            &mut visit_stack,
            &mut install_details,
            &system_packages,
            package_index,
        )?;
    }
    let packages_to_install = install_details
        .into_iter()
        .map(|(_, install_record)| install_record.repository_package)
        .collect();

    println!();
    Ok(packages_to_install)
}

// NOTE: Since this buildpack is not meant to be a replacement for a fully-featured dependency
//       manager like Apt, the dependency resolution used here is relatively simplistic. For
//       example:
//
//       - We make no attempts to handle Debian Package fields like Recommends, Suggests, Enhances, Breaks,
//         Conflicts, or Replaces. Since the build happens in a container, if the system is put into
//         an inconsistent state, it's always possible to rebuild with a different configuration.
//
//       - When adding dependencies for a package requested for install we ignore any alternative
//         package names given for a dependency (i.e.; those separated by the `|` symbol).
//
//       - No attempts are made to find the most appropriate version to install for a package given
//         any version constraints listed for packages. The latest available version will always be
//         chosen.
//
//       - Any packages that are already on the system will not be installed.
//
//       The dependency solving done here is mostly for convenience. Any transitive packages added
//       will be reported to the user and, if they aren't correct, the user may disable this dependency
//       resolution on a per-package basis and specify a more appropriate set of packages.
fn visit(
    package: &str,
    skip_dependencies: bool,
    visit_stack: &mut IndexSet<String>,
    install_details: &mut IndexMap<String, InstallRecord>,
    system_packages: &HashMap<String, Control>,
    package_index: &PackageIndex,
) -> BuildpackResult<()> {
    if let Some(system_package) = system_packages.get(package) {
        // only show this message if the package is a top-level dependency
        if visit_stack.is_empty() {
            println!(
                "  ! Skipping {package} because {name}@{version} is already installed on the system (consider removing {package} from your project.toml configuration for this buildpack)",
                name = system_package.package,
                version = system_package.version
            );
        }
        return Ok(());
    }

    if let Some(install_record) = install_details.get(package) {
        // only show this message if the package is a top-level dependency
        if visit_stack.is_empty() {
            println!(
                "  ! Skipping {package} because {name}@{version} was already installed as a dependency of {top_level_dependency} (consider removing {package} from your project.toml configuration for this buildpack)",
                name = install_record.repository_package.name,
                version = install_record.repository_package.version,
                top_level_dependency = install_record.dependency_path.first().expect("The dependency path should always have at least 1 item")
            );
        }
        return Ok(());
    }

    if let Some(package) = package_index.get_highest_available_version(package) {
        if visit_stack.is_empty() {
            println!(
                "  Adding {name}@{version}",
                name = package.name,
                version = package.version
            );
        } else {
            println!(
                "  Adding {name}@{version} [from {path}]",
                name = package.name,
                version = package.version,
                path = visit_stack
                    .iter()
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ← ")
            );
        }
        install_details.insert(
            package.name.to_string(),
            InstallRecord {
                repository_package: package.clone(),
                dependency_path: visit_stack.iter().cloned().collect(),
            },
        );
        visit_stack.insert(package.name.to_string());

        if !skip_dependencies {
            for dependency in package.get_dependencies() {
                // Don't bother looking at any dependencies we've already seen or that are already
                // on the system because it'll just cause a bunch of noisy output. We only want
                // output details about requested packages and any transitive dependencies added.
                let already_processed = system_packages.contains_key(dependency)
                    || install_details.contains_key(dependency);
                if !already_processed {
                    visit(
                        dependency,
                        skip_dependencies,
                        visit_stack,
                        install_details,
                        system_packages,
                        package_index,
                    )?;
                }
            }
        }

        visit_stack.shift_remove(&package.name);
    } else {
        let virtual_package_provider =
            get_provider_for_virtual_package(package, package_index, visit_stack)?;
        visit(
            virtual_package_provider.name.as_str(),
            skip_dependencies,
            visit_stack,
            install_details,
            system_packages,
            package_index,
        )?;
    }

    Ok(())
}

fn get_provider_for_virtual_package<'a>(
    package: &str,
    package_index: &'a PackageIndex,
    visit_stack: &IndexSet<String>,
) -> BuildpackResult<&'a RepositoryPackage> {
    let providers = package_index.get_providers(package);
    Ok(match providers.iter().collect::<Vec<_>>().as_slice() {
        [providing_package] => {
            package_index
                .get_highest_available_version(providing_package)
                .inspect(|repository_package| {
                    // only show this message if the package is a top-level dependency
                    if visit_stack.is_empty() {
                        println!(
                            "  ! Virtual package {package} is provided by {name}@{version} (consider replacing {package} for {name} in your project.toml configuration for this buildpack)",
                            name = repository_package.name,
                            version = repository_package.version
                        );
                    }
                })
                .ok_or(DeterminePackagesToInstallError::PackageNotFound(package.to_string()))
        }
        [] => Err(DeterminePackagesToInstallError::PackageNotFound(
            package.to_string(),
        )),
        _ => Err(
            DeterminePackagesToInstallError::VirtualPackageMustBeSpecified(
                package.to_string(),
                providers
                    .into_iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>(),
            ),
        ),
    }?)
}

#[derive(Debug)]
pub(crate) enum DeterminePackagesToInstallError {
    ReadSystemPackages(PathBuf, std::io::Error),
    ParseSystemPackage(PathBuf, String, apt_parser::errors::APTError),
    PackageNotFound(String),
    VirtualPackageMustBeSpecified(String, HashSet<String>),
}

impl From<DeterminePackagesToInstallError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: DeterminePackagesToInstallError) -> Self {
        Self::BuildpackError(DebianPackagesBuildpackError::DeterminePackagesToInstall(
            value,
        ))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct InstallRecord {
    repository_package: RepositoryPackage,
    dependency_path: Vec<String>,
}

#[cfg(test)]
mod test {
    use super::*;

    use std::collections::BTreeSet;
    use std::str::FromStr;

    use bon::builder;

    use crate::debian::RepositoryUri;

    #[test]
    fn install_package_already_on_the_system() {
        let package_a = create_repository_package().name("package-a").call();

        let install_state = test_install_state()
            .with_package_index(vec![&package_a])
            .with_system_packages(vec![&package_a])
            .install(&package_a.name)
            .call()
            .unwrap();

        assert!(install_state.is_empty());
    }

    #[test]
    fn install_package_already_installed_as_a_dependency_by_a_previous_package() {
        let package_b = create_repository_package().name("package-b").call();

        let package_a = create_repository_package()
            .name("package-a")
            .depends(vec![&package_b])
            .call();

        let install_state = test_install_state()
            .with_package_index(vec![&package_a, &package_b])
            .with_installed(vec![&package_a, &package_b])
            .install(&package_b.name)
            .call()
            .unwrap();

        assert_eq!(
            installed_package_names(&install_state),
            vec!["package-a", "package-b"]
        );
    }

    #[test]
    fn install_virtual_package_when_there_is_only_a_single_provider() {
        let virtual_package = "virtual-package";

        let virtual_package_provider = create_repository_package()
            .name("virtual-package-provider")
            .provides(vec![virtual_package])
            .call();

        let install_state = test_install_state()
            .with_package_index(vec![&virtual_package_provider])
            .install(virtual_package)
            .call()
            .unwrap();

        assert_eq!(
            installed_package_names(&install_state),
            vec!["virtual-package-provider"]
        );
    }

    #[test]
    fn install_virtual_package_when_there_are_multiple_providers() {
        let virtual_package = "virtual-package";

        let virtual_package_provider1 = create_repository_package()
            .name("virtual-package-provider1")
            .provides(vec![virtual_package])
            .call();

        let virtual_package_provider2 = create_repository_package()
            .name("virtual-package-provider2")
            .provides(vec![virtual_package])
            .call();

        let error = test_install_state()
            .with_package_index(vec![&virtual_package_provider1, &virtual_package_provider2])
            .install(virtual_package)
            .call()
            .unwrap_err();

        if let libcnb::Error::BuildpackError(
            DebianPackagesBuildpackError::DeterminePackagesToInstall(
                DeterminePackagesToInstallError::VirtualPackageMustBeSpecified(package, providers),
            ),
        ) = error
        {
            assert_eq!(package, virtual_package);
            assert_eq!(
                providers,
                HashSet::from([
                    virtual_package_provider1.name,
                    virtual_package_provider2.name
                ])
            );
        } else {
            panic!("not the expected error: {error:?}")
        }
    }

    #[test]
    fn install_virtual_package_when_there_are_no_providers() {
        let virtual_package = "virtual-package";

        let error = test_install_state()
            .with_package_index(vec![])
            .install("virtual-package")
            .call()
            .unwrap_err();

        if let libcnb::Error::BuildpackError(
            DebianPackagesBuildpackError::DeterminePackagesToInstall(
                DeterminePackagesToInstallError::PackageNotFound(name),
            ),
        ) = error
        {
            assert_eq!(name, virtual_package.to_string());
        } else {
            panic!("not the expected error: {error:?}")
        }
    }

    #[test]
    fn install_highest_version_of_package_when_there_are_multiple_versions() {
        let package_name = "test-package";

        let package_v0 = create_repository_package()
            .name(package_name)
            .version("1.2.2-2ubuntu0.22.04.2")
            .call();

        let package_v1 = create_repository_package()
            .name(package_name)
            .version("1.2.3-2ubuntu0.22.04.2")
            .call();

        assert!(
            debversion::Version::from_str(package_v0.version.as_str()).unwrap()
                < debversion::Version::from_str(package_v1.version.as_str()).unwrap()
        );

        let install_state = test_install_state()
            .with_package_index(vec![&package_v0, &package_v1])
            .install(package_name)
            .call()
            .unwrap();

        assert_eq!(
            install_state
                .iter()
                .map(|(_, install_record)| {
                    (
                        install_record.repository_package.name.as_str(),
                        install_record.repository_package.version.as_str(),
                    )
                })
                .next()
                .unwrap(),
            (package_name, package_v1.version.as_str())
        );
    }

    #[test]
    fn install_package_and_dependencies() {
        let package_d = create_repository_package().name("package-d").call();

        let package_c = create_repository_package()
            .name("package-c")
            .pre_depends(vec![&package_d])
            .call();

        let package_b = create_repository_package()
            .name("package-b")
            .depends(vec![&package_c])
            .call();

        let package_a = create_repository_package()
            .name("package-a")
            .depends(vec![&package_b])
            .call();

        let install_state = test_install_state()
            .with_package_index(vec![&package_a, &package_b, &package_c, &package_d])
            .install(&package_a.name)
            .call()
            .unwrap();

        assert_eq!(
            installed_package_names(&install_state),
            vec!["package-a", "package-b", "package-c", "package-d"]
        );
    }

    #[test]
    fn install_package_but_skip_dependencies() {
        let package_d = create_repository_package().name("package-d").call();

        let package_c = create_repository_package()
            .name("package-c")
            .pre_depends(vec![&package_d])
            .call();

        let package_b = create_repository_package()
            .name("package-b")
            .depends(vec![&package_c])
            .call();

        let package_a = create_repository_package()
            .name("package-a")
            .depends(vec![&package_b])
            .call();

        let install_state = test_install_state()
            .with_package_index(vec![&package_a, &package_b, &package_c, &package_d])
            .install(&package_a.name)
            .skip_dependencies(true)
            .call()
            .unwrap();

        assert_eq!(installed_package_names(&install_state), vec!["package-a"]);
    }

    #[test]
    fn install_a_non_virtual_package_which_also_has_a_provider() {
        let package_a = create_repository_package().name("package-a").call();

        let package_providing_a = create_repository_package()
            .name("package-a-provider")
            .provides(vec![package_a.name.as_str()])
            .call();

        let install_state = test_install_state()
            .with_package_index(vec![&package_a, &package_providing_a])
            .install(&package_a.name)
            .call()
            .unwrap();

        assert_eq!(installed_package_names(&install_state), vec!["package-a"]);
    }

    #[test]
    fn handles_circular_dependencies() {
        let mut package_c = create_repository_package().name("package-c").call();
        let package_d = create_repository_package().name("package-d").call();

        let package_b = create_repository_package()
            .name("package-b")
            .depends(vec![&package_c, &package_d])
            .call();

        let package_a = create_repository_package()
            .name("package-a")
            .depends(vec![&package_b])
            .call();

        // because of the circular reference, we can't set it using the builder above
        package_c.depends = Some(package_a.name.clone());

        let install_state = test_install_state()
            .with_package_index(vec![&package_a, &package_b, &package_c, &package_d])
            .install(&package_a.name)
            .call()
            .unwrap();

        assert_eq!(
            installed_package_names(&install_state),
            vec!["package-a", "package-b", "package-c", "package-d"]
        );
    }

    #[builder]
    fn test_install_state(
        install: &str,
        with_package_index: Vec<&RepositoryPackage>,
        with_installed: Option<Vec<&RepositoryPackage>>,
        with_system_packages: Option<Vec<&RepositoryPackage>>,
        skip_dependencies: Option<bool>,
    ) -> BuildpackResult<IndexMap<String, InstallRecord>> {
        let package_to_install = install;

        let mut package_index = PackageIndex::default();
        for value in with_package_index {
            package_index.add_package(value.clone());
        }

        let mut install_state = with_installed
            .unwrap_or_default()
            .into_iter()
            .map(|repository_package| {
                (
                    repository_package.name.clone(),
                    InstallRecord {
                        repository_package: repository_package.clone(),
                        dependency_path: vec!["dummy-package".to_string()],
                    },
                )
            })
            .collect();

        let skip_dependencies = skip_dependencies.unwrap_or(false);

        let mut visit_stack = IndexSet::new();

        let system_packages = with_system_packages
            .unwrap_or_default()
            .into_iter()
            .map(|repository_package| {
                let name = repository_package.name.as_str();
                (
                    name.to_string(),
                    Control::from(&format!(
                        "Package: {name}\nVersion: 1.0.0\nArchitecture: test"
                    ))
                    .unwrap(),
                )
            })
            .collect();

        visit(
            package_to_install,
            skip_dependencies,
            &mut visit_stack,
            &mut install_state,
            &system_packages,
            &package_index,
        )
        .map(|()| install_state)
    }

    #[builder]
    fn create_repository_package(
        name: &str,
        version: Option<&str>,
        provides: Option<Vec<&str>>,
        depends: Option<Vec<&RepositoryPackage>>,
        pre_depends: Option<Vec<&RepositoryPackage>>,
    ) -> RepositoryPackage {
        let join_deps = |vs: Vec<&RepositoryPackage>| {
            vs.iter()
                .map(|v| v.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        RepositoryPackage {
            name: name.to_string(),
            version: version.unwrap_or("1.0.0").to_string(),
            provides: provides.map(|vs| vs.join(",")),
            repository_uri: RepositoryUri::from(""),
            sha256sum: String::new(),
            depends: depends.map(join_deps),
            pre_depends: pre_depends.map(join_deps),
            filename: String::new(),
        }
    }

    fn installed_package_names(install_state: &IndexMap<String, InstallRecord>) -> Vec<String> {
        install_state
            .iter()
            .map(|(k, _)| k.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}
