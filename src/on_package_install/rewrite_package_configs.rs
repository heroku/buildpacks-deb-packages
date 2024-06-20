use std::fs::{read_to_string, write};
use std::path::Path;

use walkdir::{DirEntry, WalkDir};

use crate::on_package_install::rewrite_package_configs::RewritePackageConfigsError::{
    ReadPackageConfig, WritePackageConfig,
};

type Result<T> = std::result::Result<T, RewritePackageConfigsError>;

// Modify any pkg-config files to use a prefix field that references the install directory instead
// of whatever hardcoded value might be present.
//
// See: https://manpages.ubuntu.com/manpages/noble/en/man5/pc.5.html
pub(crate) fn rewrite_package_configs(install_path: &Path) -> Result<()> {
    WalkDir::new(install_path)
        .into_iter()
        .flatten()
        .filter(is_package_config)
        .try_for_each(|entry| rewrite_package_config(entry.path(), install_path))
}

fn is_package_config(entry: &DirEntry) -> bool {
    matches!((
        entry.path().parent().and_then(|p| p.file_name()),
        entry.path().extension()
    ), (Some(parent), Some(ext)) if parent == "pkgconfig" && ext == "pc")
}

fn rewrite_package_config(package_config: &Path, install_path: &Path) -> Result<()> {
    let contents = read_to_string(package_config).map_err(ReadPackageConfig)?;

    let new_contents = contents
        .lines()
        .map(|line| {
            if let Some(prefix_value) = line.strip_prefix("prefix=") {
                format!(
                    "prefix={}",
                    install_path
                        .join(prefix_value.trim_start_matches('/'))
                        .to_string_lossy()
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    write(package_config, new_contents).map_err(WritePackageConfig)
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum RewritePackageConfigsError {
    ReadPackageConfig(std::io::Error),
    WritePackageConfig(std::io::Error),
}
