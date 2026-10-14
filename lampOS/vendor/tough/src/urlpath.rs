//! This module contains utilities for mapping URL paths to local Paths.
use std::path::PathBuf;
use url::Url;

/// Converts a file URL into a file path.
/// Needed because `url.to_file_path()` will decode any percent encoding, which could restore path
/// traversal characters, and `url.path()` roots paths to '/' on Windows.
pub trait SafeUrlPath {
    /// Returns the path component of a URL as a filesystem path.
    fn safe_url_filepath(&self) -> PathBuf;
}

#[cfg(windows)]
impl SafeUrlPath for Url {
    fn safe_url_filepath(&self) -> PathBuf {
        let url_path = self.path();

        // Windows filepaths when written as `file://` URLs have path components prefixed with a /.
        PathBuf::from(if let Some(stripped) = url_path.strip_prefix('/') {
            stripped
        } else {
            url_path
        })
    }
}

#[cfg(unix)]
impl SafeUrlPath for Url {
    fn safe_url_filepath(&self) -> PathBuf {
        PathBuf::from(self.path())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::encode_filename;
    use std::path::PathBuf;

    fn manifest_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn test_safe_simple() {
        let cargo_toml = manifest_dir().join("Cargo.toml");
        let cargo_toml_url = Url::from_file_path(&cargo_toml)
            .expect("Could not create URL from Cargo.toml filepath");

        let safe_url_path = cargo_toml_url.safe_url_filepath();

        assert_eq!(cargo_toml, safe_url_path);
        assert!(safe_url_path.is_absolute());
    }

    #[test]
    fn test_safe_traversals() {
        let url_base = Url::from_directory_path(manifest_dir())
            .expect("Could not create URL from CARGO_MANIFEST_DIR");

        let escaped_test_path = encode_filename("a/../b/././c/..");
        let traversal_url = url_base.join(&escaped_test_path).unwrap_or_else(|_| {
            panic!(
                "Could not create URL from unusual traversal path '{}' + '{}'",
                url_base, escaped_test_path
            )
        });

        assert_eq!(
            manifest_dir().join("a%2F..%2Fb%2F.%2F.%2Fc%2F.."),
            traversal_url.safe_url_filepath(),
        );
    }
}
