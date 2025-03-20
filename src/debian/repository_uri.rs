use valuable::Valuable;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Valuable)]
pub(crate) struct RepositoryUri(String);

impl RepositoryUri {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for RepositoryUri {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_as_str() {
        let repository = RepositoryUri("http://archive.ubuntu.com/ubuntu".to_string());
        assert_eq!(repository.as_str(), "http://archive.ubuntu.com/ubuntu");
    }

    #[test]
    fn test_from_string() {
        let repository = RepositoryUri("http://archive.ubuntu.com/ubuntu".to_string());
        let repository_from_string = RepositoryUri::from("http://archive.ubuntu.com/ubuntu");
        assert_eq!(repository, repository_from_string);
    }
}
