use fs_err::read_to_string;
use std::collections::HashMap;

use apt_parser::Control;
use indexmap::{IndexMap, IndexSet};

use crate::config::RequestedPackage;
use crate::debian::{PackageIndex, RepositoryPackage};
use crate::determine_packages_to_install::DeterminePackagesToInstallError::{
    PackageNotFound, ParseSystemPackage, ReadSystemPackages, VirtualPackageMustBeSpecified,
};

type Result<T> = std::result::Result<T, DeterminePackagesToInstallError>;

pub(crate) fn determine_packages_to_install(
    package_index: &PackageIndex,
    requested_packages: IndexSet<RequestedPackage>,
) -> Result<Vec<RepositoryPackage>> {
    println!("## Determining packages to install");
    println!();

    let system_packages = read_to_string("/var/lib/dpkg/status")
        .map_err(ReadSystemPackages)?
        .trim()
        .split("\n\n")
        .map(|control_data| {
            Control::from(control_data)
                .map_err(ParseSystemPackage)
                .map(|control| (control.package.to_string(), control))
        })
        .collect::<Result<HashMap<_, _>>>()?;

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
) -> Result<()> {
    if let Some(system_package) = system_packages.get(package) {
        println!(
            "  ! Skipping {package} because {name}@{version} is already installed on the system (consider removing {package} from your project.toml configuration for this buildpack)",
            name = system_package.package,
            version = system_package.version
        );
        return Ok(());
    }

    if let Some(install_record) = install_details.get(package) {
        println!(
            "  ! Skipping {package} because {name}@{version} was already installed as a dependency of {top_level_dependency} (consider removing {package} from your project.toml configuration for this buildpack)",
            name = install_record.repository_package.name,
            version = install_record.repository_package.version,
            top_level_dependency = install_record.dependency_path.first().expect("The dependency path should always have at least 1 item")

        );
        return Ok(());
    }

    if let Some(provides) = package_index.get_virtual_package_providers(package) {
        return match provides.as_slice() {
            [repository_package] => {
                println!("  ! Virtual package {package} is provided by {name}@{version} (consider replacing {package} for {name} in your project.toml configuration for this buildpack)", name = repository_package.name, version = repository_package.version);
                visit(
                    &repository_package.name,
                    skip_dependencies,
                    visit_stack,
                    install_details,
                    system_packages,
                    package_index,
                )
            }
            _ => Err(VirtualPackageMustBeSpecified(
                provides.iter().map(Clone::clone).collect(),
            )),
        };
    }

    let repository_package = package_index
        .get_highest_available_version(package)
        .ok_or(PackageNotFound)?;

    if visit_stack.is_empty() {
        println!(
            "  Adding {name}@{version}",
            name = repository_package.name,
            version = repository_package.version
        );
    } else {
        println!(
            "  Adding {name}@{version} [from {path}]",
            name = repository_package.name,
            version = repository_package.version,
            path = visit_stack
                .iter()
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join(" ← ")
        );
    }
    install_details.insert(
        package.to_string(),
        InstallRecord {
            repository_package: repository_package.clone(),
            dependency_path: visit_stack.iter().cloned().collect(),
        },
    );
    visit_stack.insert(package.to_string());

    if !skip_dependencies {
        for dependency in repository_package.get_dependencies() {
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

    visit_stack.shift_remove(package);

    Ok(())
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum DeterminePackagesToInstallError {
    ReadSystemPackages(std::io::Error),
    ParseSystemPackage(apt_parser::errors::APTError),
    PackageNotFound,
    VirtualPackageMustBeSpecified(Vec<RepositoryPackage>),
}

struct InstallRecord {
    repository_package: RepositoryPackage,
    dependency_path: Vec<String>,
}
