use reqwest::Url;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use toml_edit::Value;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub(crate) struct DownloadUrl(Url);

impl DownloadUrl {
    pub(crate) fn filename(&self) -> Option<&str> {
        self.0
            .path_segments()
            .and_then(|mut paths| paths.next_back())
            .and_then(|path| path.strip_suffix(".deb"))
            .and_then(|path| if path.is_empty() { None } else { Some(path) })
    }
}

impl Display for DownloadUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DownloadUrl {
    type Err = ParseDownloadUrlError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(value).map_err(|e| ParseDownloadUrlError::InvalidUrl {
            url: value.into(),
            reason: e.to_string(),
        })?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(ParseDownloadUrlError::InvalidUrl {
                url: value.into(),
                reason: "must start with `http://` or `https://`".into(),
            });
        }
        match std::path::Path::new(url.path()).extension() {
            Some(ext) => {
                if !ext.eq_ignore_ascii_case("deb") {
                    return Err(ParseDownloadUrlError::InvalidUrl {
                        url: value.into(),
                        reason: "must end with `.deb`".into(),
                    });
                }
            }
            None => {
                return Err(ParseDownloadUrlError::InvalidUrl {
                    url: value.into(),
                    reason: "file doesn't have an extension".into(),
                });
            }
        }
        Ok(DownloadUrl(url))
    }
}

impl TryFrom<&Value> for DownloadUrl {
    type Error = ParseDownloadUrlError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        if let Some(value) = value.as_str() {
            DownloadUrl::from_str(value)
        } else {
            Err(ParseDownloadUrlError::UnexpectedTomlValue(value.clone()))
        }
    }
}

#[derive(Debug)]
pub(crate) enum ParseDownloadUrlError {
    InvalidUrl { url: String, reason: String },
    UnexpectedTomlValue(Value),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_http_url() {
        let url = "http://example.com/package-1.2.3.deb";
        let download_url = DownloadUrl::from_str(url).unwrap();
        assert_eq!(download_url.to_string(), url);
    }

    #[test]
    fn test_valid_https_url() {
        let url = "https://example.com/path/to/package-1.2.3.deb";
        let download_url = DownloadUrl::from_str(url).unwrap();
        assert_eq!(download_url.to_string(), url);
    }

    #[test]
    fn test_invalid_scheme_file() {
        let url = "file:///path/to/package.deb";
        let error = DownloadUrl::from_str(url).unwrap_err();
        match error {
            ParseDownloadUrlError::InvalidUrl { url: u, reason } => {
                assert_eq!(u, url);
                assert_eq!(reason, "must start with `http://` or `https://`");
            }
            ParseDownloadUrlError::UnexpectedTomlValue(_) => panic!("Expected InvalidUrl error"),
        }
    }

    #[test]
    fn test_missing_deb_extension() {
        let url = "https://example.com/package.tar.gz";
        let error = DownloadUrl::from_str(url).unwrap_err();
        match error {
            ParseDownloadUrlError::InvalidUrl { url: u, reason } => {
                assert_eq!(u, url);
                assert_eq!(reason, "must end with `.deb`");
            }
            ParseDownloadUrlError::UnexpectedTomlValue(_) => panic!("Expected InvalidUrl error"),
        }
    }

    #[test]
    fn test_missing_extension_entirely() {
        let url = "https://example.com/package";
        let error = DownloadUrl::from_str(url).unwrap_err();
        match error {
            ParseDownloadUrlError::InvalidUrl { url: u, reason } => {
                assert_eq!(u, url);
                assert_eq!(reason, "file doesn't have an extension");
            }
            ParseDownloadUrlError::UnexpectedTomlValue(_) => panic!("Expected InvalidUrl error"),
        }
    }

    #[test]
    fn test_just_deb() {
        let url = "https://example.com/.deb";
        let error = DownloadUrl::from_str(url).unwrap_err();
        match error {
            ParseDownloadUrlError::UnexpectedTomlValue(_) => {
                panic!("Expected InvalidUrl error");
            }
            ParseDownloadUrlError::InvalidUrl { reason, .. } => {
                assert_eq!(reason, "file doesn't have an extension");
            }
        }
    }

    #[test]
    fn test_relative_url_without_base() {
        let url = "not a url";
        let error = DownloadUrl::from_str(url).unwrap_err();
        match error {
            ParseDownloadUrlError::InvalidUrl { url: u, reason } => {
                assert_eq!(u, url);
                assert_eq!(reason, "relative URL without a base");
            }
            ParseDownloadUrlError::UnexpectedTomlValue(_) => panic!("Expected InvalidUrl error"),
        }
    }

    #[test]
    fn test_filename_simple() {
        let url = "https://example.com/package-1.2.3.deb";
        let download_url = DownloadUrl::from_str(url).unwrap();
        assert_eq!(download_url.filename(), Some("package-1.2.3"));
    }

    #[test]
    fn test_filename_with_path() {
        let url = "https://example.com/path/to/my-package.deb";
        let download_url = DownloadUrl::from_str(url).unwrap();
        assert_eq!(download_url.filename(), Some("my-package"));
    }

    #[test]
    fn test_filename_with_complex_name() {
        let url = "https://github.com/wkhtmltopdf/packaging/releases/download/0.12.6.1-2/wkhtmltox_0.12.6.1-2.jammy_amd64.deb";
        let download_url = DownloadUrl::from_str(url).unwrap();
        assert_eq!(
            download_url.filename(),
            Some("wkhtmltox_0.12.6.1-2.jammy_amd64")
        );
    }

    #[test]
    fn test_try_from_valid_toml_string() {
        let value = toml_edit::value("https://example.com/package.deb");
        let download_url = DownloadUrl::try_from(value.as_value().unwrap()).unwrap();
        assert_eq!(download_url.to_string(), "https://example.com/package.deb");
    }

    #[test]
    fn test_try_from_invalid_toml_type_integer() {
        let value = toml_edit::value(42);
        let error = DownloadUrl::try_from(value.as_value().unwrap()).unwrap_err();
        match error {
            ParseDownloadUrlError::UnexpectedTomlValue(_) => {}
            ParseDownloadUrlError::InvalidUrl { .. } => {
                panic!("Expected UnexpectedTomlValue error")
            }
        }
    }

    #[test]
    fn test_display_implementation() {
        let url = "https://example.com/package.deb";
        let download_url = DownloadUrl::from_str(url).unwrap();
        assert_eq!(format!("{download_url}"), url);
    }

    #[test]
    fn test_clone_and_equality() {
        let url1 = DownloadUrl::from_str("https://example.com/package.deb").unwrap();
        let url2 = url1.clone();
        assert_eq!(url1, url2);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::HashSet;

        let url1 = DownloadUrl::from_str("https://example.com/package.deb").unwrap();
        let url2 = DownloadUrl::from_str("https://example.com/package.deb").unwrap();

        let mut set = HashSet::new();
        set.insert(url1);
        assert!(set.contains(&url2));
    }
}
