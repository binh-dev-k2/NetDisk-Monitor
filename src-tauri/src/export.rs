/// Validate a single lowercase `.csv` filename and bounded UTF-8 content.
///
/// # Errors
/// Rejects invalid filenames and content exceeding 16 MiB.
#[allow(clippy::case_sensitive_file_extension_comparisons)] // Lowercase .csv is the existing API contract.
pub fn validate_export(filename: &str, content: &str) -> Result<(), String> {
    let stem = filename
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        });
    if filename.is_empty()
        || filename.contains(['/', '\\', ':', '\0'])
        || !filename.ends_with(".csv")
        || filename.starts_with('.')
        || reserved
        || filename.contains(['<', '>', '"', '|', '?', '*'])
        || filename.chars().any(char::is_control)
    {
        return Err("Invalid CSV filename".to_owned());
    }
    if content.len() > 16 * 1024 * 1024 {
        return Err("CSV exceeds the 16 MiB export limit".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_export;
    #[test]
    fn export_cannot_escape_downloads() {
        for name in [
            "../secret.csv",
            "..\\secret.csv",
            "C:\\secret.csv",
            "secret.exe",
            "",
            "CON.csv",
            "com1.report.csv",
            "LPT9.csv",
            "bad?name.csv",
        ] {
            assert!(validate_export(name, "").is_err(), "{name}");
        }
        assert!(validate_export("NetDisk_Report.csv", "a,b\n").is_ok());
    }
    #[test]
    fn export_is_bounded() {
        assert!(validate_export("report.csv", &"x".repeat(16 * 1024 * 1024 + 1)).is_err());
    }
}
