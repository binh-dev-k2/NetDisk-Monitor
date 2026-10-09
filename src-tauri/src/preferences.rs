use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
};

#[must_use]
pub fn encode(flags: u8, opacity: u8) -> [u8; 8] {
    let mut bytes = [b'N', b'D', b'M', b'P', 1, flags, opacity, 0];
    bytes[7] = bytes[..7].iter().fold(0xA5, |sum, b| sum ^ b);
    bytes
}

#[must_use]
pub fn decode(bytes: &[u8]) -> Option<(u8, u8)> {
    if bytes.len() != 8
        || &bytes[..5] != b"NDMP\x01"
        || !(35..=92).contains(&bytes[6])
        || bytes[7] != bytes[..7].iter().fold(0xA5, |sum, b| sum ^ b)
    {
        return None;
    }
    Some((bytes[5], bytes[6]))
}

/// Load the v1 preferences, or return `None` when no file exists.
///
/// # Errors
/// Returns filesystem errors or `InvalidData` for malformed preferences.
pub fn load(directory: &Path) -> io::Result<Option<(u8, u8)>> {
    match File::open(directory.join("preferences-v1.bin")) {
        Ok(file) => read_preferences(file).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_preferences(reader: impl Read) -> io::Result<(u8, u8)> {
    // One extra byte distinguishes exact v1 payloads from oversized files without unbounded reads.
    let mut bytes = Vec::with_capacity(9);
    reader.take(9).read_to_end(&mut bytes)?;
    decode(&bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid preferences file"))
}

/// Replace preferences through a synchronized temporary file. The caller serializes writes.
///
/// # Errors
/// Returns directory creation, write, synchronization, or rename errors.
pub fn save(directory: &Path, bytes: &[u8; 8]) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let temporary = directory.join("preferences-v1.tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, directory.join("preferences-v1.bin"))
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use std::{
        fs, io,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn malformed_preferences_read_at_most_nine_bytes() {
        let mut reader = io::Cursor::new(vec![0; 1024]);
        assert_eq!(
            super::read_preferences(&mut reader).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(reader.position(), 9);
    }
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "netdisk-preferences-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn load_rejects_short_and_oversized_files_and_save_replaces() {
        let dir = Temp::new();
        let path = dir.0.join("preferences-v1.bin");
        assert_eq!(super::load(&dir.0).unwrap(), None);
        for length in [0, 7, 9, 1024] {
            let mut bytes = encode(1, 72).to_vec();
            bytes.resize(length, 0);
            fs::write(&path, bytes).unwrap();
            assert_eq!(
                super::load(&dir.0).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        super::save(&dir.0, &encode(1, 72)).unwrap();
        assert_eq!(super::load(&dir.0).unwrap(), Some((1, 72)));
        super::save(&dir.0, &encode(2, 92)).unwrap();
        assert_eq!(super::load(&dir.0).unwrap(), Some((2, 92)));
    }
    #[test]
    fn settings_round_trip_and_corruption_is_rejected() {
        for flags in 0..=255 {
            let encoded = encode(flags, 72);
            assert_eq!(decode(&encoded), Some((flags, 72)));
            for index in 0..8 {
                let mut corrupt = encoded;
                corrupt[index] ^= 1;
                assert_eq!(decode(&corrupt), None);
            }
        }
    }
    #[test]
    fn invalid_opacity_and_version_are_rejected() {
        assert!(decode(&encode(255, 0)).is_none());
        assert!(decode(&[]).is_none());
    }
}
