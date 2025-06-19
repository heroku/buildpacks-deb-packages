use crate::debian::{ArchitectureName, RepositoryUri, Source, UnsupportedArchitectureNameError};
use toml_edit::{Table, Value};

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
        let uri = table
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ParseCustomSourceError::MissingUri(table.clone()))?
            .into();

        let mut suites: Vec<String> = vec![];
        if let Some(array) = table.get("suites").and_then(|v| v.as_array()) {
            for suite in array {
                suites.push(
                    suite
                        .as_str()
                        .ok_or_else(|| {
                            ParseCustomSourceError::UnexpectedTomlValue(
                                table.clone(),
                                suite.clone(),
                            )
                        })?
                        .into(),
                );
            }
        }

        if suites.is_empty() {
            return Err(ParseCustomSourceError::MissingSuites(table.clone()));
        }

        let mut components: Vec<String> = vec![];
        if let Some(array) = table.get("components").and_then(|v| v.as_array()) {
            for component in array {
                components.push(
                    component
                        .as_str()
                        .ok_or_else(|| {
                            ParseCustomSourceError::UnexpectedTomlValue(
                                table.clone(),
                                component.clone(),
                            )
                        })?
                        .into(),
                );
            }
        }

        if components.is_empty() {
            return Err(ParseCustomSourceError::MissingComponents(table.clone()));
        }

        let mut arch: Vec<ArchitectureName> = vec![];
        if let Some(array) = table.get("arch").and_then(|v| v.as_array()) {
            for arch_value in array {
                arch.push(
                    arch_value
                        .as_str()
                        .ok_or_else(|| {
                            ParseCustomSourceError::UnexpectedTomlValue(
                                table.clone(),
                                arch_value.clone(),
                            )
                        })?
                        .parse()
                        .map_err(|e| {
                            ParseCustomSourceError::InvalidArchitectureName(table.clone(), e)
                        })?,
                );
            }
        }

        if arch.is_empty() {
            return Err(ParseCustomSourceError::MissingArchitectureNames(
                table.clone(),
            ));
        }

        let signed_by = table
            .get("signed_by")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ParseCustomSourceError::MissingSignedBy(table.clone()))?
            .into();

        Ok(CustomSource {
            arch,
            components,
            suites,
            uri,
            signed_by,
        })
    }
}

#[derive(Debug)]
pub(crate) enum ParseCustomSourceError {
    MissingUri(Table),
    MissingSignedBy(Table),
    MissingSuites(Table),
    MissingComponents(Table),
    MissingArchitectureNames(Table),
    UnexpectedTomlValue(Table, Value),
    InvalidArchitectureName(Table, UnsupportedArchitectureNameError),
}
