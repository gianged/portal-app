use crate::errors::SharedError;

/// Cap on a sanitized upload filename, in characters.
pub const FILENAME_MAX: usize = 128;

/// Normalizes a client-supplied upload filename to a single safe path segment:
/// keeps the last `/`/`\` component, strips control characters and quotes, and
/// caps the length while preserving the extension. Safe to embed in a storage
/// key and a `Content-Disposition` header.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when nothing usable remains (empty,
/// `.`, or `..`).
pub fn sanitize_filename(raw: &str) -> Result<String, SharedError> {
    let last = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .collect::<String>();
    if last.is_empty() || last == "." || last == ".." {
        return Err(SharedError::Validation("Filename must not be empty".into()));
    }
    if last.chars().count() <= FILENAME_MAX {
        return Ok(last);
    }
    // Over-long name: keep the extension, truncate the stem.
    let (stem, ext) = match last.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.chars().count() < FILENAME_MAX => {
            (stem.to_owned(), format!(".{ext}"))
        }
        _ => (last.clone(), String::new()),
    };
    let keep = FILENAME_MAX - ext.chars().count();
    Ok(format!(
        "{}{ext}",
        stem.chars().take(keep).collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::FILENAME_MAX;

    #[test]
    fn plain_name_passes_through() {
        assert_eq!(
            super::sanitize_filename("report.pdf").unwrap(),
            "report.pdf"
        );
    }

    #[test]
    fn path_components_are_stripped() {
        assert_eq!(
            super::sanitize_filename("C:\\Users\\x\\evil.exe").unwrap(),
            "evil.exe"
        );
        assert_eq!(
            super::sanitize_filename("../../etc/passwd").unwrap(),
            "passwd"
        );
    }

    #[test]
    fn control_chars_and_quotes_are_removed() {
        assert_eq!(super::sanitize_filename("a\"b\r\n.txt").unwrap(), "ab.txt");
    }

    #[test]
    fn empty_and_dot_names_are_rejected() {
        assert!(super::sanitize_filename("").is_err());
        assert!(super::sanitize_filename("   ").is_err());
        assert!(super::sanitize_filename("..").is_err());
        assert!(super::sanitize_filename("uploads/").is_err());
    }

    #[test]
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn long_names_keep_their_extension() {
        let long = format!("{}.tar.gz", "a".repeat(300));
        let out = super::sanitize_filename(&long).unwrap();
        assert_eq!(out.chars().count(), FILENAME_MAX);
        assert!(out.ends_with(".gz"));
    }
}
