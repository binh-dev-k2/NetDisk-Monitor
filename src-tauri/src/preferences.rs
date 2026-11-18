use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System = 0,
    Dark = 1,
    Light = 2,
}

impl ThemeMode {
    const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::System),
            1 => Some(Self::Dark),
            2 => Some(Self::Light),
            _ => None,
        }
    }

    const fn as_byte(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredPreferences {
    pub flags: u8,
    pub opacity: u8,
    pub overlay_visible: bool,
    pub taskbar_enabled: bool,
    pub theme: ThemeMode,
    pub overlay_position: Option<(i32, i32)>,
}

impl Default for StoredPreferences {
    fn default() -> Self {
        Self {
            // Default overlay metrics: down (1) + up (2) + ram (64) + pinned (128).
            flags: 0b1100_0011,
            opacity: 72,
            overlay_visible: true,
            taskbar_enabled: false,
            theme: ThemeMode::System,
            overlay_position: None,
        }
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut last_err = None;
    for attempt in 0..3 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied && attempt < 2 => {
                last_err = Some(error);
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::other("Rename failed")))
}

#[must_use]
pub fn encode_v2(prefs: &StoredPreferences) -> [u8; 24] {
    let mut bytes = [0_u8; 24];
    bytes[..5].copy_from_slice(b"NDMP\x02");
    bytes[5] = prefs.flags;
    bytes[6] = prefs.opacity.clamp(35, 92);
    let mut bits = u8::from(prefs.overlay_visible)
        | (u8::from(prefs.taskbar_enabled) << 1)
        | (u8::from(prefs.overlay_position.is_some()) << 2)
        | (prefs.theme.as_byte() << 3);
    bits &= 0b0001_1111;
    bytes[7] = bits;
    if let Some((x, y)) = prefs.overlay_position {
        bytes[8..12].copy_from_slice(&x.to_le_bytes());
        bytes[12..16].copy_from_slice(&y.to_le_bytes());
    }
    let checksum = fnv1a_64(&bytes[..16]).to_le_bytes();
    bytes[16..24].copy_from_slice(&checksum);
    bytes
}

#[must_use]
pub fn decode_v2(bytes: &[u8]) -> Option<StoredPreferences> {
    if bytes.len() != 24 || &bytes[..5] != b"NDMP\x02" || !(35..=92).contains(&bytes[6]) {
        return None;
    }
    let bits = bytes[7];
    if (bits & 0b1110_0000) != 0 {
        return None;
    }
    let theme = ThemeMode::from_byte((bits >> 3) & 0b11)?;
    let expected = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
    if expected != fnv1a_64(&bytes[..16]) {
        return None;
    }
    let has_pos = bits & 4 != 0;
    let overlay_position = if has_pos {
        let x = i32::from_le_bytes(bytes[8..12].try_into().ok()?);
        let y = i32::from_le_bytes(bytes[12..16].try_into().ok()?);
        Some((x, y))
    } else if bytes[8..16].iter().any(|&b| b != 0) {
        return None;
    } else {
        None
    };
    Some(StoredPreferences {
        flags: bytes[5],
        opacity: bytes[6],
        overlay_visible: bits & 1 != 0,
        taskbar_enabled: bits & 2 != 0,
        theme,
        overlay_position,
    })
}

/// Load v2 preferences, falling back to v1 if v2 does not yet exist.
///
/// # Errors
/// Returns filesystem errors or `InvalidData` for malformed preferences.
pub fn load_stored(directory: &Path) -> io::Result<Option<StoredPreferences>> {
    match File::open(directory.join("preferences-v2.bin")) {
        Ok(file) => read_preferences_v2(file).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Ok(load(directory)?.map(|(flags, opacity)| StoredPreferences {
                flags,
                opacity,
                ..StoredPreferences::default()
            }))
        }
        Err(error) => Err(error),
    }
}

fn read_preferences_v2(reader: impl Read) -> io::Result<StoredPreferences> {
    let mut bytes = Vec::with_capacity(25);
    reader.take(25).read_to_end(&mut bytes)?;
    decode_v2(&bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid v2 preferences file"))
}

/// Replace v2 preferences atomically via a synchronized temporary file.
///
/// # Errors
/// Returns directory creation, write, synchronization, or rename errors.
pub fn save_stored(directory: &Path, prefs: &StoredPreferences) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let bytes = encode_v2(prefs);
    let temporary = directory.join("preferences-v2.tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    rename_with_retry(&temporary, &directory.join("preferences-v2.bin"))
}

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
    rename_with_retry(&temporary, &directory.join("preferences-v1.bin"))
}

/// Removes obsolete `preferences-v1.bin` only once `preferences-v2.bin` exists.
///
/// # Errors
/// Returns filesystem errors other than `NotFound` if removal fails.
pub fn remove_legacy_v1_if_migrated(directory: &Path) -> io::Result<bool> {
    if !directory.join("preferences-v2.bin").is_file() {
        return Ok(false);
    }
    match fs::remove_file(directory.join("preferences-v1.bin")) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
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

    #[test]
    fn v2_preferences_round_trip_and_reject_corruption() {
        let prefs = super::StoredPreferences {
            flags: 0b1010_0101,
            opacity: 85,
            overlay_visible: false,
            taskbar_enabled: true,
            theme: super::ThemeMode::Light,
            overlay_position: Some((-140, 24)),
        };
        let encoded = super::encode_v2(&prefs);
        assert_eq!(super::decode_v2(&encoded), Some(prefs));
        for index in 0..24 {
            let mut corrupt = encoded;
            corrupt[index] ^= 1;
            assert_eq!(super::decode_v2(&corrupt), None);
        }
        assert!(super::decode_v2(&[]).is_none());
        let mut oversized = encoded.to_vec();
        oversized.push(0);
        assert!(super::decode_v2(&oversized).is_none());
    }

    #[test]
    fn v2_loader_migrates_from_v1_and_persists_independently() {
        let dir = Temp::new();
        super::save(&dir.0, &encode(0b0101_0101, 60)).unwrap();
        let migrated = super::load_stored(&dir.0).unwrap().unwrap();
        assert_eq!(migrated.flags, 0b0101_0101);
        assert_eq!(migrated.opacity, 60);
        assert!(migrated.overlay_visible);
        assert!(!migrated.taskbar_enabled);
        assert_eq!(migrated.theme, super::ThemeMode::System);
        assert_eq!(migrated.overlay_position, None);

        let updated = super::StoredPreferences {
            flags: 0b1100_0011,
            opacity: 77,
            overlay_visible: false,
            taskbar_enabled: true,
            theme: super::ThemeMode::Dark,
            overlay_position: Some((500, 12)),
        };
        super::save_stored(&dir.0, &updated).unwrap();
        assert_eq!(super::load_stored(&dir.0).unwrap(), Some(updated));
    }

    #[test]
    fn legacy_v1_removal_requires_existing_v2_file() {
        let dir = Temp::new();
        super::save(&dir.0, &encode(1, 72)).unwrap();
        assert!(!super::remove_legacy_v1_if_migrated(&dir.0).unwrap());
        assert!(dir.0.join("preferences-v1.bin").exists());

        super::save_stored(&dir.0, &super::StoredPreferences::default()).unwrap();
        assert!(super::remove_legacy_v1_if_migrated(&dir.0).unwrap());
        assert!(!dir.0.join("preferences-v1.bin").exists());
        assert!(!super::remove_legacy_v1_if_migrated(&dir.0).unwrap());
    }
}
