use serde::Serialize;
use std::fmt::Display;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize)]
pub(crate) struct RepositoryUri(String);

impl From<&str> for RepositoryUri {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl Display for RepositoryUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for RepositoryUri {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_as_ref() {
        let repository = RepositoryUri("http://archive.ubuntu.com/ubuntu".to_string());
        assert_eq!(repository.as_ref(), "http://archive.ubuntu.com/ubuntu");
    }

    #[test]
    fn test_from_string() {
        let repository = RepositoryUri("http://archive.ubuntu.com/ubuntu".to_string());
        let repository_from_string = RepositoryUri::from("http://archive.ubuntu.com/ubuntu");
        assert_eq!(repository, repository_from_string);
    }
}
