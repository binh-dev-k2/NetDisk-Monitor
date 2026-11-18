//! Bounded inspection and metadata reporting for on-disk and in-memory app storage.
use crate::{
    diagnostics::{DIAGNOSTIC_FILES, MAX_DIAGNOSTICS_BYTES},
    storage::{DAY_RETENTION_SLOTS, HISTORY_FILE_BYTES, MINUTE_RETENTION_SLOTS},
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const SCAN_MAX_ENTRIES: usize = 4_096;
const SCAN_MAX_DEPTH: usize = 10;
const PREFERENCES_BUDGET_BYTES: u64 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearTarget {
    Diagnostics,
    Caches,
    History,
    AllClearable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageItem {
    pub id: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub max_bytes: Option<u64>,
    pub record_count: Option<usize>,
    pub clearable: bool,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCacheStats {
    pub icon_cache_count: usize,
    pub icon_cache_limit: usize,
    pub terminated_process_count: usize,
    pub terminated_process_limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageOverview {
    pub app_data_dir: String,
    pub web_view_data_dir: Option<String>,
    pub total_app_managed_bytes: u64,
    pub max_app_managed_bytes: u64,
    pub total_web_view_bytes: u64,
    pub items: Vec<StorageItem>,
    pub memory: MemoryCacheStats,
}

fn file_entry_size(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path)
        .ok()
        .filter(|meta| meta.file_type().is_file())
        .map(|meta| meta.len())
}

fn file_size(path: &Path) -> u64 {
    file_entry_size(path).unwrap_or(0)
}

pub(crate) fn is_reparse_or_symlink(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if meta.is_dir() && (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return true;
        }
    }
    false
}

/// Counts regular file bytes under `root` without following symlinks and within strict bounds.
#[must_use]
pub fn scan_dir_bounded(root: &Path, max_entries: usize, max_depth: usize) -> (u64, usize) {
    let Ok(root_meta) = fs::symlink_metadata(root) else {
        return (0, 0);
    };
    if is_reparse_or_symlink(&root_meta) {
        return (0, 0);
    }
    if !root_meta.file_type().is_dir() {
        return if root_meta.file_type().is_file() {
            (root_meta.len(), 1)
        } else {
            (0, 0)
        };
    }
    let mut stack = vec![(root.to_path_buf(), 0_usize)];
    let (mut total_bytes, mut total_files, mut visited) = (0_u64, 0_usize, 0_usize);
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if visited >= max_entries {
                return (total_bytes, total_files);
            }
            visited += 1;
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.file_type().is_file() {
                total_bytes = total_bytes.saturating_add(meta.len());
                total_files += 1;
            } else if meta.file_type().is_dir()
                && !is_reparse_or_symlink(&meta)
                && depth < max_depth
            {
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    (total_bytes, total_files)
}

fn build_history_item(app_data_dir: &Path, counts: (usize, usize)) -> StorageItem {
    let history_path = app_data_dir.join("history-v1.bin");
    StorageItem {
        id: "history".into(),
        label: "Lịch sử lưu lượng (vòng đệm)".into(),
        path: history_path.to_string_lossy().into_owned(),
        bytes: file_size(&history_path),
        max_bytes: Some(HISTORY_FILE_BYTES),
        record_count: Some(counts.0.saturating_add(counts.1)),
        clearable: true,
        detail: format!(
            "{} bản ghi phút (tối đa {MINUTE_RETENTION_SLOTS}), {} bản ghi ngày (tối đa {DAY_RETENTION_SLOTS}) · Cố định kích thước",
            counts.0, counts.1
        ),
    }
}

fn build_diagnostics_item(app_data_dir: &Path) -> StorageItem {
    let diag_dir = app_data_dir.join("diagnostics");
    let mut bytes = 0_u64;
    let mut count = 0_usize;
    for name in DIAGNOSTIC_FILES {
        if let Some(size) = file_entry_size(&diag_dir.join(name)) {
            count += 1;
            bytes = bytes.saturating_add(size);
        }
    }
    StorageItem {
        id: "diagnostics".into(),
        label: "Nhật ký chẩn đoán (xoay vòng)".into(),
        path: diag_dir.to_string_lossy().into_owned(),
        bytes,
        max_bytes: Some(MAX_DIAGNOSTICS_BYTES),
        record_count: Some(count),
        clearable: true,
        detail: "Gồm events.jsonl, events.1.jsonl và last-fatal.txt · Tự xoay vòng tối đa 516 KB"
            .into(),
    }
}

fn build_preferences_item(app_data_dir: &Path) -> StorageItem {
    let pref_v2 = app_data_dir.join("preferences-v2.bin");
    let pref_v1 = app_data_dir.join("preferences-v1.bin");
    let lock = app_data_dir.join("host.lock");
    let files = [&pref_v2, &pref_v1, &lock];
    let bytes = files.iter().map(|p| file_size(p)).sum();
    let count = files.iter().filter(|p| p.is_file()).count();
    let legacy_note = if pref_v1.is_file() {
        " · Phát hiện bản v1 cũ có thể dọn"
    } else {
        ""
    };
    StorageItem {
        id: "preferences".into(),
        label: "Cấu hình người dùng & khóa phiên".into(),
        path: pref_v2.to_string_lossy().into_owned(),
        bytes,
        max_bytes: Some(PREFERENCES_BUDGET_BYTES),
        record_count: Some(count),
        clearable: false,
        detail: format!("Lưu vị trí thanh nổi, giao diện, taskbar (được giữ khi dọn){legacy_note}"),
    }
}

/// Builds a complete overview of managed disk files and bounded memory caches.
#[must_use]
pub fn build_overview(
    app_data_dir: &Path,
    webview_dir: Option<&Path>,
    history_counts: (usize, usize),
    memory: MemoryCacheStats,
) -> StorageOverview {
    let history = build_history_item(app_data_dir, history_counts);
    let diagnostics = build_diagnostics_item(app_data_dir);
    let preferences = build_preferences_item(app_data_dir);
    let total_app_managed_bytes = history
        .bytes
        .saturating_add(diagnostics.bytes)
        .saturating_add(preferences.bytes);
    let max_app_managed_bytes = HISTORY_FILE_BYTES
        .saturating_add(MAX_DIAGNOSTICS_BYTES)
        .saturating_add(PREFERENCES_BUDGET_BYTES);
    let mut items = vec![history, diagnostics, preferences];
    let distinct_wv = webview_dir.filter(|wv| *wv != app_data_dir);
    let total_web_view_bytes = if let Some(wv) = distinct_wv {
        let (bytes, files) = scan_dir_bounded(wv, SCAN_MAX_ENTRIES, SCAN_MAX_DEPTH);
        items.push(StorageItem {
            id: "webview".into(),
            label: "Bộ nhớ đệm giao diện WebView2".into(),
            path: wv.to_string_lossy().into_owned(),
            bytes,
            max_bytes: None,
            record_count: Some(files),
            clearable: false,
            detail: "Cache trình duyệt nội bộ do hệ điều hành quản lý khi UI đang mở".into(),
        });
        bytes
    } else {
        0
    };
    StorageOverview {
        app_data_dir: app_data_dir.to_string_lossy().into_owned(),
        web_view_data_dir: distinct_wv.map(|p| p.to_string_lossy().into_owned()),
        total_app_managed_bytes,
        max_app_managed_bytes,
        total_web_view_bytes,
        items,
        memory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(std::path::PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "netdisk-storage-info-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scans_real_files_and_separates_managed_from_webview_totals() {
        let root = TestDir::new();
        let app_dir = root.0.join("roaming");
        let diag_dir = app_dir.join("diagnostics");
        let local_dir = root.0.join("local");
        fs::create_dir_all(&diag_dir).unwrap();
        fs::create_dir_all(local_dir.join("EBWebView/Default")).unwrap();
        fs::write(app_dir.join("history-v1.bin"), vec![1_u8; 1024]).unwrap();
        fs::write(diag_dir.join("events.jsonl"), b"{\"a\":1}\n").unwrap();
        fs::write(app_dir.join("preferences-v2.bin"), [0_u8; 24]).unwrap();
        fs::write(local_dir.join("EBWebView/Default/Cache"), vec![2_u8; 4096]).unwrap();

        let overview = build_overview(
            &app_dir,
            Some(&local_dir),
            (12, 3),
            MemoryCacheStats {
                icon_cache_count: 8,
                icon_cache_limit: 512,
                terminated_process_count: 4,
                terminated_process_limit: 512,
            },
        );
        assert_eq!(overview.total_app_managed_bytes, 1024 + 8 + 24);
        assert_eq!(overview.total_web_view_bytes, 4096);
        assert_eq!(overview.items.len(), 4);
        assert_eq!(overview.items[0].record_count, Some(15));
        assert!(overview.items[0].clearable);
        assert!(!overview.items[2].clearable);
        assert!(!overview.items[3].clearable);
    }

    #[test]
    fn bounded_scan_handles_missing_and_empty_directories() {
        let missing = Path::new("Z:/nonexistent-netdisk-test-path");
        assert_eq!(scan_dir_bounded(missing, 100, 5), (0, 0));

        let empty = TestDir::new();
        assert_eq!(scan_dir_bounded(&empty.0, 100, 5), (0, 0));
    }
}
