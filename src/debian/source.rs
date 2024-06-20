use crate::debian::{ArchitectureName, RepositoryUri};

// NOTE: This is meant to be similar in structure to the Deb822 Source Format described at
//       https://manpages.ubuntu.com/manpages/jammy/man5/sources.list.5.html#deb822-style%20format.
//
//       Some differences between this and documented Deb822 Source Format are:
//       - Type is omitted because we aren't supporting building from source (deb-src), only pre-compiled binaries (deb)
//       - Only one URI is allowed even though the source format says URIs is an array
//       - Enabled is always true, so it's omitted here
//       - Only the Signed-By option is supported
#[derive(Debug)]
pub(crate) struct Source {
    pub(crate) arch: ArchitectureName,
    pub(crate) components: Vec<String>,
    pub(crate) signed_by: String,
    pub(crate) suites: Vec<String>,
    pub(crate) uri: RepositoryUri,
}

impl Source {
    pub(crate) fn new<I, S>(
        uri: S,
        suites: I,
        components: I,
        signed_by: S,
        arch: ArchitectureName,
    ) -> Source
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Source {
            components: components.into_iter().map(Into::into).collect(),
            signed_by: signed_by.into(),
            suites: suites.into_iter().map(Into::into).collect(),
            uri: RepositoryUri::from(uri.into()),
            arch,
        }
    }
}
