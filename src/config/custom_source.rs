use crate::debian::{ArchitectureName, RepositoryUri, Source};
use toml_edit::Table;

// Very similar in structure to a `Source` **except** it allows for multiple architectures
// to be specified as configuration.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CustomSource {
    pub(crate) arch: Vec<ArchitectureName>,
    pub(crate) components: Vec<String>,
    pub(crate) suites: Vec<String>,
    pub(crate) uri: RepositoryUri,
    pub(crate) signed_by: String,
}

impl CustomSource {
    pub(crate) fn to_sources(&self) -> Vec<Source> {
        self.arch
            .iter()
            .map(|arch| Source {
                uri: self.uri.clone(),
                suites: self.suites.clone(),
                components: self.components.clone(),
                signed_by: self.signed_by.clone(),
                arch: arch.clone(),
            })
            .collect()
    }
}

impl TryFrom<&Table> for CustomSource {
    type Error = ParseCustomSourceError;

    fn try_from(table: &Table) -> Result<Self, Self::Error> {
        // todo: this needs more robust error handling

        let uri = table.get("uri").and_then(|v| v.as_str()).unwrap().into();

        let mut suites: Vec<String> = vec![];
        if let Some(array) = table.get("suites").and_then(|v| v.as_array()) {
            for suite in array {
                suites.push(suite.as_str().unwrap().into());
            }
        }

        let mut components: Vec<String> = vec![];
        if let Some(array) = table.get("components").and_then(|v| v.as_array()) {
            for component in array {
                components.push(component.as_str().unwrap().into());
            }
        }

        let mut arch: Vec<ArchitectureName> = vec![];
        if let Some(array) = table.get("arch").and_then(|v| v.as_array()) {
            for arch_value in array {
                arch.push(arch_value.as_str().unwrap().parse().unwrap());
            }
        }

        let gpg_key = table
            .get("signed_by")
            .and_then(|v| v.as_str())
            .unwrap()
            .into();

        Ok(CustomSource {
            uri,
            suites,
            components,
            arch,
            signed_by: gpg_key,
        })
    }
}

#[derive(Debug)]
pub(crate) enum ParseCustomSourceError {}
