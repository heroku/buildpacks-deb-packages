use crate::config::RequestedPackage;
use crate::debian::{PackageIndex, RepositoryPackage};
use crate::{BuildpackResult, DebianPackagesBuildpackError};
use apt_parser::Control;
use bullet_stream::{global::print, style};
use edit_distance::edit_distance;
use indexmap::IndexSet;
use serde::Serialize;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::fs::read_to_string;
use std::path::PathBuf;
use tracing::instrument;

#[instrument(skip_all)]
pub(crate) fn determine_packages_to_install(
    package_index: &PackageIndex,
    requested_packages: IndexSet<RequestedPackage>,
) -> BuildpackResult<Vec<RepositoryPackage>> {
    print::header("Determining packages to install");
    print::bullet("Collecting system install information");
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
                .map(SystemPackage::from)
        })
        .collect::<Result<IndexSet<_>, _>>()?;

    let mut packages_marked_for_install = IndexSet::new();

    for requested_package in requested_packages {
        print::bullet(format!(
            "Determining install requirements for requested package {package}",
            package = style::value(requested_package.name.as_str())
        ));
        let mut visit_stack = IndexSet::new();
        let mut package_notifications = IndexSet::new();

        visit(
            requested_package.name.as_str(),
            requested_package.skip_dependencies,
            requested_package.force,
            &system_packages,
            package_index,
            &mut packages_marked_for_install,
            &mut visit_stack,
            &mut package_notifications,
        )?;

        if package_notifications.is_empty() {
            print::sub_bullet("Nothing to add");
        } else {
            for package_notification in package_notifications {
                print::sub_bullet(package_notification.to_string());
            }
        }
    }

    let packages_to_install = packages_marked_for_install
        .into_iter()
        .map(|package_marked_for_install| package_marked_for_install.repository_package)
        .collect::<Vec<_>>();

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
#[allow(clippy::too_many_arguments)]
fn visit(
    package: &str,
    skip_dependencies: bool,
    force_if_installed_on_system: bool,
    system_packages: &IndexSet<SystemPackage>,
    package_index: &PackageIndex,
    packages_marked_for_install: &mut IndexSet<PackageMarkedForInstall>,
    visit_stack: &mut IndexSet<String>,
    package_notifications: &mut IndexSet<PackageNotification>,
) -> BuildpackResult<()> {
    if let Some(system_package) = find_system_package_by_name(package, system_packages) {
        // When a package is already installed on the system we skip installing it. However, there are
        // cases where a package might be installed in the build image but not the run image. There's
        // no easy method to check for that condition at build-time so we allow the user to "force" the
        // installation of this package via configuration.
        if !force_if_installed_on_system {
            package_notifications.insert(PackageNotification::AlreadyInstalledOnSystem {
                system_package_name: system_package.package_name.clone(),
                system_package_version: system_package.package_version.clone(),
            });
            return Ok(());
        }
    }

    if let Some(package_marked_for_install) =
        find_package_marked_for_install_by_name(package, packages_marked_for_install)
    {
        package_notifications.insert(PackageNotification::AlreadyInstalledByOtherPackage {
            installed_package: package_marked_for_install.repository_package.clone(),
            installed_by: package_marked_for_install.requested_by.clone(),
        });
        return Ok(());
    }

    if let Some(repository_package) = package_index.get_highest_available_version(package) {
        packages_marked_for_install.insert(PackageMarkedForInstall {
            repository_package: repository_package.clone(),
            requested_by: visit_stack.first().cloned().unwrap_or(package.to_string()),
        });

        package_notifications.insert(PackageNotification::Added {
            repository_package: repository_package.clone(),
            dependency_path: visit_stack.iter().cloned().collect(),
            forced_install: force_if_installed_on_system,
        });

        visit_stack.insert(repository_package.name.clone());

        if !skip_dependencies {
            for dependency in repository_package.get_dependencies() {
                if should_visit_dependency(dependency, system_packages, packages_marked_for_install)
                {
                    visit(
                        dependency,
                        skip_dependencies,
                        force_if_installed_on_system,
                        system_packages,
                        package_index,
                        packages_marked_for_install,
                        visit_stack,
                        package_notifications,
                    )?;
                }
            }
        }

        visit_stack.shift_remove(&repository_package.name);
    } else {
        let virtual_package_provider =
            get_provider_for_virtual_package(package, package_index, package_notifications)?;

        visit_stack.insert(package.to_string());

        visit(
            virtual_package_provider.name.as_str(),
            skip_dependencies,
            force_if_installed_on_system,
            system_packages,
            package_index,
            packages_marked_for_install,
            visit_stack,
            package_notifications,
        )?;

        visit_stack.shift_remove(package);
    }

    Ok(())
}

fn get_provider_for_virtual_package<'a>(
    package: &str,
    package_index: &'a PackageIndex,
    package_install_details: &mut IndexSet<PackageNotification>,
) -> BuildpackResult<&'a RepositoryPackage> {
    let providers = package_index.get_providers(package);
    Ok(match providers.iter().collect::<Vec<_>>().as_slice() {
        [providing_package] => package_index
            .get_highest_available_version(providing_package)
            .inspect(|repository_package| {
                package_install_details.insert(
                    PackageNotification::VirtualPackageHasOnlyOneImplementor {
                        requested_package: package.to_string(),
                        implementor: (*repository_package).clone(),
                    },
                );
            })
            .ok_or(DeterminePackagesToInstallError::PackageNotFound(
                package.to_string(),
                find_suggested_packages(package, package_index),
            )),
        [] => Err(DeterminePackagesToInstallError::PackageNotFound(
            package.to_string(),
            find_suggested_packages(package, package_index),
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

fn find_system_package_by_name<'a>(
    package_name: &str,
    system_packages: &'a IndexSet<SystemPackage>,
) -> Option<&'a SystemPackage> {
    system_packages
        .iter()
        .find(|system_package| system_package.package_name == package_name)
}

fn find_package_marked_for_install_by_name<'a>(
    package_name: &str,
    packages_marked_for_install: &'a IndexSet<PackageMarkedForInstall>,
) -> Option<&'a PackageMarkedForInstall> {
    packages_marked_for_install
        .iter()
        .find(|package_marked_for_install| {
            package_name == package_marked_for_install.repository_package.name
        })
}

fn should_visit_dependency(
    dependency: &str,
    system_packages: &IndexSet<SystemPackage>,
    packages_marked_for_install: &IndexSet<PackageMarkedForInstall>,
) -> bool {
    // Don't bother looking at any dependencies we've already seen or that are already
    // on the system because it'll just cause a bunch of noisy output. We only want
    // output details about requested packages and any transitive dependencies added.
    matches!(
        (
            find_system_package_by_name(dependency, system_packages),
            find_package_marked_for_install_by_name(dependency, packages_marked_for_install)
        ),
        (None, None)
    )
}

fn find_suggested_packages(package: &str, package_index: &PackageIndex) -> Vec<String> {
    let mut suggested_packages = package_index
        .get_package_names()
        .iter()
        .filter_map(|name| {
            let distance = edit_distance(package, name);
            // only take suggestions between 1 and 3 edit distances away
            if (1..=3).contains(&distance) {
                Some((distance, (*name).to_string()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    suggested_packages.sort_by_key(|(distance, _)| *distance);

    suggested_packages
        .into_iter()
        .take(5)
        .map(|(_, name)| name)
        .collect()
}

#[derive(Debug)]
pub(crate) enum DeterminePackagesToInstallError {
    ReadSystemPackages(PathBuf, std::io::Error),
    ParseSystemPackage(PathBuf, String, apt_parser::errors::APTError),
    PackageNotFound(String, Vec<String>),
    VirtualPackageMustBeSpecified(String, HashSet<String>),
}

impl From<DeterminePackagesToInstallError> for libcnb::Error<DebianPackagesBuildpackError> {
    fn from(value: DeterminePackagesToInstallError) -> Self {
        Self::BuildpackError(DebianPackagesBuildpackError::DeterminePackagesToInstall(
            Box::new(value),
        ))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum PackageNotification {
    Added {
        repository_package: RepositoryPackage,
        dependency_path: Vec<String>,
        forced_install: bool,
    },
    AlreadyInstalledOnSystem {
        system_package_name: String,
        system_package_version: String,
    },
    AlreadyInstalledByOtherPackage {
        installed_package: RepositoryPackage,
        installed_by: String,
    },
    VirtualPackageHasOnlyOneImplementor {
        requested_package: String,
        implementor: RepositoryPackage,
    },
}

impl Display for PackageNotification {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageNotification::Added {
                repository_package,
                dependency_path,
                forced_install,
            } => {
                if dependency_path.is_empty() {
                    let added = format!(
                        "Adding {name_with_version}",
                        name_with_version = style::value(format!(
                            "{name}@{version}",
                            name = repository_package.name,
                            version = repository_package.version
                        ))
                    );
                    if *forced_install {
                        write!(f, "{added} (forced)")
                    } else {
                        write!(f, "{added}")
                    }
                } else {
                    write!(
                        f,
                        "Adding {name_with_version} [from {path}]",
                        name_with_version = style::value(format!(
                            "{name}@{version}",
                            name = repository_package.name,
                            version = repository_package.version
                        )),
                        path = dependency_path
                            .iter()
                            .rev()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" ← ")
                    )
                }
            }
            PackageNotification::AlreadyInstalledOnSystem {
                system_package_name,
                system_package_version,
            } => {
                write!(
                    f,
                    "Skipping {package} because {name_with_version} is already installed on the system",
                    package = style::value(system_package_name),
                    name_with_version =
                        style::value(format!("{system_package_name}@{system_package_version}")),
                )
            }
            PackageNotification::AlreadyInstalledByOtherPackage {
                installed_package,
                installed_by,
            } => {
                write!(
                    f,
                    "Skipping {package} because {name_with_version} was already installed as a dependency of {installed_by}",
                    package = style::value(&installed_package.name),
                    name_with_version = style::value(format!(
                        "{name}@{version}",
                        name = &installed_package.name,
                        version = &installed_package.version
                    )),
                    installed_by = style::value(installed_by),
                )
            }
            PackageNotification::VirtualPackageHasOnlyOneImplementor {
                requested_package,
                implementor,
            } => {
                write!(
                    f,
                    "Virtual package {package} is provided by {name_with_version}",
                    package = style::value(requested_package),
                    name_with_version = style::value(format!(
                        "{name}@{version}",
                        name = &implementor.name,
                        version = &implementor.version
                    )),
                )
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct PackageMarkedForInstall {
    repository_package: RepositoryPackage,
    requested_by: String,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize)]
struct SystemPackage {
    package_name: String,
    package_version: String,
}

impl From<Control> for SystemPackage {
    fn from(value: Control) -> Self {
        Self {
            package_name: value.package,
            package_version: value.version,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::str::FromStr;

    use bon::builder;

    use crate::debian::RepositoryUri;

    #[test]
    fn install_package_already_on_the_system() {
        let package_a = create_repository_package().name("package-a").call();

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a])
            .with_system_packages(IndexSet::from([create_system_package()
                .package_name(&package_a.name)
                .call()]))
            .install(&package_a.name)
            .call()
            .unwrap();

        assert!(new_packages_marked_for_install.is_empty());

        assert_eq!(
            package_notifications,
            IndexSet::from([PackageNotification::AlreadyInstalledOnSystem {
                system_package_name: package_a.name.clone(),
                system_package_version: DEFAULT_VERSION.to_string(),
            }])
        );
    }

    #[test]
    fn install_package_already_installed_as_a_dependency_by_a_previous_package() {
        let package_b = create_repository_package().name("package-b").call();

        let package_a = create_repository_package()
            .name("package-a")
            .depends(vec![&package_b])
            .call();

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a, &package_b])
            .with_installed(IndexSet::from([
                create_package_marked_for_install()
                    .repository_package(&package_a)
                    .call(),
                create_package_marked_for_install()
                    .repository_package(&package_b)
                    .requested_by(&package_a.name)
                    .call(),
            ]))
            .install(&package_b.name)
            .call()
            .unwrap();

        assert!(new_packages_marked_for_install.is_empty());

        assert_eq!(
            package_notifications,
            IndexSet::from([PackageNotification::AlreadyInstalledByOtherPackage {
                installed_package: package_b,
                installed_by: package_a.name,
            }])
        );
    }

    #[test]
    fn install_virtual_package_when_there_is_only_a_single_provider() {
        let virtual_package = "virtual-package";

        let virtual_package_provider = create_repository_package()
            .name("virtual-package-provider")
            .provides(vec![virtual_package])
            .call();

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&virtual_package_provider])
            .install(virtual_package)
            .call()
            .unwrap();

        assert_eq!(
            new_packages_marked_for_install,
            IndexSet::from([create_package_marked_for_install()
                .repository_package(&virtual_package_provider)
                .requested_by(virtual_package)
                .call()])
        );

        assert_eq!(
            package_notifications,
            IndexSet::from([
                PackageNotification::VirtualPackageHasOnlyOneImplementor {
                    requested_package: virtual_package.to_string(),
                    implementor: virtual_package_provider.clone()
                },
                PackageNotification::Added {
                    repository_package: virtual_package_provider,
                    dependency_path: vec![virtual_package.to_string()],
                    forced_install: false,
                },
            ])
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
            DebianPackagesBuildpackError::DeterminePackagesToInstall(boxed_error),
        ) = error
        {
            if let DeterminePackagesToInstallError::VirtualPackageMustBeSpecified(
                package,
                providers,
            ) = *boxed_error
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
                panic!("not the expected error: {boxed_error:?}");
            }
        } else {
            panic!("not the expected error: {error:?}");
        }
    }

    #[test]
    fn install_virtual_package_when_there_are_no_providers() {
        let virtual_package = "virtual-package";

        let package_with_edit_distance_1 = create_repository_package()
            .name(&format!("{virtual_package}1"))
            .call();
        let package_with_virtual_package_edit_distance_2_and_3 = create_repository_package()
            .name("another-virtual-package-provider")
            .provides(vec![
                &format!("{virtual_package}12"),
                &format!("{virtual_package}123"),
            ])
            .call();
        let package_with_edit_distance_4 = create_repository_package()
            .name(&format!("{virtual_package}1234"))
            .call();

        let error = test_install_state()
            .with_package_index(vec![
                &package_with_edit_distance_1,
                &package_with_virtual_package_edit_distance_2_and_3,
                &package_with_edit_distance_4,
            ])
            .install(virtual_package)
            .call()
            .unwrap_err();

        if let libcnb::Error::BuildpackError(
            DebianPackagesBuildpackError::DeterminePackagesToInstall(boxed_error),
        ) = error
        {
            if let DeterminePackagesToInstallError::PackageNotFound(name, suggestions) =
                *boxed_error
            {
                assert_eq!(name, virtual_package.to_string());
                assert_eq!(
                    suggestions,
                    vec![
                        format!("{virtual_package}1"),
                        format!("{virtual_package}12"),
                        format!("{virtual_package}123"),
                    ]
                );
            } else {
                panic!("not the expected error: {boxed_error:?}");
            }
        } else {
            panic!("not the expected error: {error:?}")
        }
    }

    #[test]
    fn install_package_that_does_not_exist_returns_suggestions_between_edit_distance_1_and_3() {
        let non_existent_package = "non-existent-package";

        let package_with_edit_distance_1 = create_repository_package()
            .name(&format!("{non_existent_package}1"))
            .call();
        let package_with_virtual_package_edit_distance_2_and_3 = create_repository_package()
            .name("another-virtual-package-provider")
            .provides(vec![
                &format!("{non_existent_package}12"),
                &format!("{non_existent_package}123"),
            ])
            .call();
        let package_with_edit_distance_4 = create_repository_package()
            .name(&format!("{non_existent_package}1234"))
            .call();

        let error = test_install_state()
            .with_package_index(vec![
                &package_with_edit_distance_1,
                &package_with_virtual_package_edit_distance_2_and_3,
                &package_with_edit_distance_4,
            ])
            .install(non_existent_package)
            .call()
            .unwrap_err();

        if let libcnb::Error::BuildpackError(
            DebianPackagesBuildpackError::DeterminePackagesToInstall(boxed_error),
        ) = error
        {
            if let DeterminePackagesToInstallError::PackageNotFound(name, suggestions) =
                *boxed_error
            {
                assert_eq!(name, non_existent_package.to_string());
                assert_eq!(
                    suggestions,
                    vec![
                        format!("{non_existent_package}1"),
                        format!("{non_existent_package}12"),
                        format!("{non_existent_package}123"),
                    ]
                );
            } else {
                panic!("not the expected error: {boxed_error:?}");
            }
        } else {
            panic!("not the expected error: {error:?}")
        }
    }

    #[test]
    fn install_package_that_does_not_exist_returns_max_5_suggestions_between_edit_distance_1_and_3()
    {
        let non_existent_package = "non-existent-package";

        let package_with_edit_distance_1 = create_repository_package()
            .name(&format!("{non_existent_package}1"))
            .call();
        let another_package_with_edit_distance_1 = create_repository_package()
            .name(&format!("{non_existent_package}a"))
            .call();
        let package_with_virtual_package_edit_distance_2_and_3 = create_repository_package()
            .name("another-virtual-package-provider")
            .provides(vec![
                &format!("{non_existent_package}12"),
                &format!("{non_existent_package}123"),
            ])
            .call();
        let another_package_with_virtual_package_edit_distance_2_and_3 =
            create_repository_package()
                .name("another-virtual-package-provider")
                .provides(vec![
                    &format!("{non_existent_package}ab"),
                    &format!("{non_existent_package}abc"),
                ])
                .call();
        let package_with_edit_distance_4 = create_repository_package()
            .name(&format!("{non_existent_package}1234"))
            .call();

        let error = test_install_state()
            .with_package_index(vec![
                &package_with_edit_distance_1,
                &package_with_virtual_package_edit_distance_2_and_3,
                &package_with_edit_distance_4,
                &another_package_with_edit_distance_1,
                &another_package_with_virtual_package_edit_distance_2_and_3,
            ])
            .install(non_existent_package)
            .call()
            .unwrap_err();

        if let libcnb::Error::BuildpackError(
            DebianPackagesBuildpackError::DeterminePackagesToInstall(boxed_error),
        ) = error
        {
            if let DeterminePackagesToInstallError::PackageNotFound(name, suggestions) =
                *boxed_error
            {
                assert_eq!(name, non_existent_package.to_string());
                assert_eq!(
                    suggestions,
                    vec![
                        format!("{non_existent_package}1"),
                        format!("{non_existent_package}a"),
                        format!("{non_existent_package}12"),
                        format!("{non_existent_package}ab"),
                        format!("{non_existent_package}123"),
                    ]
                );
            } else {
                panic!("not the expected error: {boxed_error:?}");
            }
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

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_v0, &package_v1])
            .install(package_name)
            .call()
            .unwrap();

        assert_eq!(
            new_packages_marked_for_install,
            IndexSet::from([create_package_marked_for_install()
                .repository_package(&package_v1)
                .call()])
        );

        assert_eq!(
            package_notifications,
            IndexSet::from([PackageNotification::Added {
                repository_package: package_v1,
                dependency_path: vec![],
                forced_install: false,
            }])
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

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a, &package_b, &package_c, &package_d])
            .install(&package_a.name)
            .call()
            .unwrap();

        assert_eq!(
            new_packages_marked_for_install,
            IndexSet::from([
                create_package_marked_for_install()
                    .repository_package(&package_a)
                    .call(),
                create_package_marked_for_install()
                    .repository_package(&package_b)
                    .requested_by(&package_a.name)
                    .call(),
                create_package_marked_for_install()
                    .repository_package(&package_c)
                    .requested_by(&package_a.name)
                    .call(),
                create_package_marked_for_install()
                    .repository_package(&package_d)
                    .requested_by(&package_a.name)
                    .call()
            ])
        );

        assert_eq!(
            package_notifications,
            IndexSet::from([
                PackageNotification::Added {
                    repository_package: package_a.clone(),
                    dependency_path: vec![],
                    forced_install: false,
                },
                PackageNotification::Added {
                    repository_package: package_b.clone(),
                    dependency_path: vec![package_a.name.clone()],
                    forced_install: false,
                },
                PackageNotification::Added {
                    repository_package: package_c.clone(),
                    dependency_path: vec![package_a.name.clone(), package_b.name.clone()],
                    forced_install: false,
                },
                PackageNotification::Added {
                    repository_package: package_d,
                    dependency_path: vec![
                        package_a.name.clone(),
                        package_b.name.clone(),
                        package_c.name.clone()
                    ],
                    forced_install: false,
                }
            ])
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

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a, &package_b, &package_c, &package_d])
            .install(&package_a.name)
            .skip_dependencies(true)
            .call()
            .unwrap();

        assert_eq!(
            new_packages_marked_for_install,
            IndexSet::from([create_package_marked_for_install()
                .repository_package(&package_a)
                .call(),])
        );

        assert_eq!(
            package_notifications,
            IndexSet::from([PackageNotification::Added {
                repository_package: package_a,
                dependency_path: vec![],
                forced_install: false,
            }])
        );
    }

    #[test]
    fn install_a_non_virtual_package_which_also_has_a_provider() {
        let package_a = create_repository_package().name("package-a").call();

        let package_providing_a = create_repository_package()
            .name("package-a-provider")
            .provides(vec![package_a.name.as_str()])
            .call();

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a, &package_providing_a])
            .install(&package_a.name)
            .call()
            .unwrap();

        assert_eq!(
            new_packages_marked_for_install,
            IndexSet::from([create_package_marked_for_install()
                .repository_package(&package_a)
                .call()])
        );

        assert_eq!(
            package_notifications,
            IndexSet::from([PackageNotification::Added {
                repository_package: package_a,
                dependency_path: vec![],
                forced_install: false,
            }])
        );
    }

    #[test]
    fn handles_circular_dependencies() {
        let package_d = create_repository_package().name("package-d").call();

        let mut package_c = create_repository_package().name("package-c").call();

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

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a, &package_b, &package_c, &package_d])
            .install(&package_a.name)
            .call()
            .unwrap();

        assert_eq!(
            new_packages_marked_for_install,
            IndexSet::from([
                create_package_marked_for_install()
                    .repository_package(&package_a)
                    .call(),
                create_package_marked_for_install()
                    .repository_package(&package_b)
                    .requested_by(&package_a.name)
                    .call(),
                create_package_marked_for_install()
                    .repository_package(&package_c)
                    .requested_by(&package_a.name)
                    .call(),
                create_package_marked_for_install()
                    .repository_package(&package_d)
                    .requested_by(&package_a.name)
                    .call()
            ])
        );

        assert_eq!(
            package_notifications,
            IndexSet::from([
                PackageNotification::Added {
                    repository_package: package_a.clone(),
                    dependency_path: vec![],
                    forced_install: false,
                },
                PackageNotification::Added {
                    repository_package: package_b.clone(),
                    dependency_path: vec![package_a.name.clone()],
                    forced_install: false,
                },
                PackageNotification::Added {
                    repository_package: package_c.clone(),
                    dependency_path: vec![package_a.name.clone(), package_b.name.clone()],
                    forced_install: false,
                },
                PackageNotification::Added {
                    repository_package: package_d,
                    dependency_path: vec![package_a.name.clone(), package_b.name.clone()],
                    forced_install: false,
                },
            ])
        );
    }

    #[test]
    fn handle_virtual_package_with_one_implementor_that_also_exists_on_the_system() {
        let libvips = "libvips";

        let libvips42t64 = create_repository_package()
            .name("libvips42t64")
            .provides(vec![libvips])
            .call();

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_system_packages(IndexSet::from([create_system_package()
                .package_name(&libvips42t64.name)
                .call()]))
            .with_package_index(vec![&libvips42t64])
            .install(libvips)
            .call()
            .unwrap();

        assert!(new_packages_marked_for_install.is_empty());

        assert_eq!(
            package_notifications,
            IndexSet::from([
                PackageNotification::VirtualPackageHasOnlyOneImplementor {
                    requested_package: libvips.to_string(),
                    implementor: libvips42t64.clone()
                },
                PackageNotification::AlreadyInstalledOnSystem {
                    system_package_name: libvips42t64.name.clone(),
                    system_package_version: libvips42t64.version.clone(),
                }
            ])
        );
    }

    #[test]
    fn handle_virtual_package_with_one_implementor_that_also_was_installed_by_a_previous_package() {
        let libvips = "libvips";

        let libvips42t64 = create_repository_package()
            .name("libvips42t64")
            .provides(vec![libvips])
            .call();

        let package_a = create_repository_package()
            .name("package-a")
            .depends(vec![&libvips42t64])
            .call();

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a, &libvips42t64])
            .with_installed(IndexSet::from([create_package_marked_for_install()
                .repository_package(&libvips42t64)
                .requested_by(&package_a.name)
                .call()]))
            .install(libvips)
            .call()
            .unwrap();

        assert!(new_packages_marked_for_install.is_empty());

        assert_eq!(
            package_notifications,
            IndexSet::from([
                PackageNotification::VirtualPackageHasOnlyOneImplementor {
                    requested_package: libvips.to_string(),
                    implementor: libvips42t64.clone()
                },
                PackageNotification::AlreadyInstalledByOtherPackage {
                    installed_package: libvips42t64,
                    installed_by: package_a.name.clone(),
                }
            ])
        );
    }

    #[test]
    fn force_install_a_package_already_on_the_system() {
        let package_a = create_repository_package().name("package-a").call();

        let system_package_a = create_system_package().package_name(&package_a.name).call();

        let (new_packages_marked_for_install, package_notifications) = test_install_state()
            .with_package_index(vec![&package_a])
            .with_system_packages(IndexSet::from([system_package_a]))
            .install(&package_a.name)
            .force(true)
            .call()
            .unwrap();

        assert_eq!(
            new_packages_marked_for_install,
            IndexSet::from([create_package_marked_for_install()
                .repository_package(&package_a)
                .call()])
        );

        assert_eq!(
            package_notifications,
            IndexSet::from([PackageNotification::Added {
                repository_package: package_a,
                dependency_path: vec![],
                forced_install: true,
            }])
        );
    }

    #[builder]
    fn test_install_state(
        install: &str,
        with_package_index: Vec<&RepositoryPackage>,
        with_installed: Option<IndexSet<PackageMarkedForInstall>>,
        with_system_packages: Option<IndexSet<SystemPackage>>,
        skip_dependencies: Option<bool>,
        force: Option<bool>,
    ) -> BuildpackResult<(
        IndexSet<PackageMarkedForInstall>,
        IndexSet<PackageNotification>,
    )> {
        let package_to_install = install;

        let skip_dependencies = skip_dependencies.unwrap_or(false);

        let force = force.unwrap_or(false);

        let mut package_index = PackageIndex::default();
        for value in with_package_index {
            package_index.add_package(value.clone());
        }

        let with_installed = with_installed.unwrap_or_default();

        let mut packages_marked_for_install = with_installed.iter().cloned().collect();

        let system_packages = with_system_packages.unwrap_or_default();

        let mut package_notifications = IndexSet::new();

        let mut visit_stack = IndexSet::new();

        visit(
            package_to_install,
            skip_dependencies,
            force,
            &system_packages,
            &package_index,
            &mut packages_marked_for_install,
            &mut visit_stack,
            &mut package_notifications,
        )?;

        let new_packages_marked_for_install = packages_marked_for_install
            .difference(&with_installed)
            .cloned()
            .collect();

        Ok((new_packages_marked_for_install, package_notifications))
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
            version: version.unwrap_or(DEFAULT_VERSION).to_string(),
            provides: provides.map(|vs| vs.join(",")),
            repository_uri: RepositoryUri::from(""),
            sha256sum: String::new(),
            depends: depends.map(join_deps),
            pre_depends: pre_depends.map(join_deps),
            filename: String::new(),
        }
    }

    #[builder]
    fn create_package_marked_for_install(
        repository_package: &RepositoryPackage,
        requested_by: Option<&str>,
    ) -> PackageMarkedForInstall {
        PackageMarkedForInstall {
            repository_package: repository_package.clone(),
            requested_by: requested_by.unwrap_or(&repository_package.name).to_string(),
        }
    }

    #[builder]
    fn create_system_package(package_name: &str, package_version: Option<&str>) -> SystemPackage {
        SystemPackage {
            package_name: package_name.to_string(),
            package_version: package_version.unwrap_or(DEFAULT_VERSION).to_string(),
        }
    }

    const DEFAULT_VERSION: &str = "1.0.0";
}
