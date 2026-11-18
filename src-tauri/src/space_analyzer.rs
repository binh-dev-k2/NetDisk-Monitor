//! Bounded single-pass disk space analyzer and developer build-artifact detector.
use crate::storage_info::is_reparse_or_symlink;
use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 40_000;
const MAX_DEPTH: usize = 10;
const TOP_CHILDREN: usize = 20;
const TOP_FILES: usize = 15;
const TOP_ARTIFACTS: usize = 15;
const DEV_ARTIFACT_NAMES: [&str; 8] = [
    "node_modules",
    "target",
    ".next",
    "dist",
    ".venv",
    "venv",
    ".gradle",
    "__pycache__",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceNode {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub is_dir: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFileEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevArtifactEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAnalysisReport {
    pub scanned_path: String,
    pub parent_path: Option<String>,
    pub total_bytes: u64,
    pub scanned_files: usize,
    pub scanned_dirs: usize,
    pub truncated: bool,
    pub children: Vec<SpaceNode>,
    pub largest_files: Vec<LargeFileEntry>,
    pub dev_artifacts: Vec<DevArtifactEntry>,
}

fn display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_owned()
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Resolves preset aliases or validates an explicit existing non-reparse directory path.
///
/// # Errors
/// Returns a descriptive error if the preset cannot be resolved or the target is invalid.
pub fn resolve_target(target: &str) -> Result<PathBuf, String> {
    let trimmed = target.trim();
    let candidate = match trimmed {
        "home" => home_dir().ok_or("Không xác định được thư mục người dùng")?,
        "downloads" => home_dir()
            .ok_or("Không tìm thấy Downloads")?
            .join("Downloads"),
        "documents" => home_dir()
            .ok_or("Không tìm thấy Documents")?
            .join("Documents"),
        "desktop" => home_dir().ok_or("Không tìm thấy Desktop")?.join("Desktop"),
        "local_appdata" => env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|p| p.join(".cache")))
            .ok_or("Không tìm thấy LocalAppData")?,
        "" => return Err("Đường dẫn quét trống".to_owned()),
        other => PathBuf::from(other),
    };
    let meta = fs::symlink_metadata(&candidate)
        .map_err(|_| "Không thể truy cập thư mục đã chọn".to_owned())?;
    if !meta.file_type().is_dir() || is_reparse_or_symlink(&meta) {
        return Err("Chỉ hỗ trợ phân tích thư mục thực hợp lệ".to_owned());
    }
    Ok(candidate)
}

fn record_large_file(files: &mut Vec<LargeFileEntry>, name: &str, path: &Path, bytes: u64) {
    if bytes == 0 {
        return;
    }
    if files.len() < TOP_FILES || files.last().is_some_and(|tail| bytes > tail.bytes) {
        files.push(LargeFileEntry {
            name: name.to_owned(),
            path: display_path(path),
            bytes,
        });
        files.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
        files.truncate(TOP_FILES);
    }
}

struct ScanAccumulator {
    visited: usize,
    scanned_files: usize,
    scanned_dirs: usize,
    truncated: bool,
    largest_files: Vec<LargeFileEntry>,
    dev_artifacts: Vec<DevArtifactEntry>,
}

fn scan_subtree(start_dir: &Path, start_name: &str, acc: &mut ScanAccumulator) -> u64 {
    let start_art = if DEV_ARTIFACT_NAMES.contains(&start_name) {
        let idx = acc.dev_artifacts.len();
        acc.dev_artifacts.push(DevArtifactEntry {
            name: start_name.to_owned(),
            path: display_path(start_dir),
            bytes: 0,
        });
        Some(idx)
    } else {
        None
    };
    let mut stack = vec![(start_dir.to_path_buf(), 1_usize, start_art)];
    let mut subtree_bytes = 0_u64;
    while let Some((dir, depth, art_idx)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if acc.visited >= MAX_ENTRIES {
                acc.truncated = true;
                return subtree_bytes;
            }
            acc.visited += 1;
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if meta.file_type().is_file() {
                let len = meta.len();
                subtree_bytes = subtree_bytes.saturating_add(len);
                acc.scanned_files += 1;
                record_large_file(&mut acc.largest_files, &name, &path, len);
                if let Some(idx) = art_idx {
                    acc.dev_artifacts[idx].bytes = acc.dev_artifacts[idx].bytes.saturating_add(len);
                }
            } else if meta.file_type().is_dir()
                && !is_reparse_or_symlink(&meta)
                && depth < MAX_DEPTH
            {
                acc.scanned_dirs += 1;
                let next_art = art_idx.or_else(|| {
                    DEV_ARTIFACT_NAMES.contains(&name.as_str()).then(|| {
                        let idx = acc.dev_artifacts.len();
                        acc.dev_artifacts.push(DevArtifactEntry {
                            name,
                            path: display_path(&path),
                            bytes: 0,
                        });
                        idx
                    })
                });
                stack.push((path, depth + 1, next_art));
            }
        }
    }
    subtree_bytes
}

fn finalize_report(
    root: &Path,
    acc: &mut ScanAccumulator,
    mut children: Vec<SpaceNode>,
    total_bytes: u64,
) -> SpaceAnalysisReport {
    children.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
    if children.len() > TOP_CHILDREN {
        let rest_bytes: u64 = children.drain(TOP_CHILDREN..).map(|n| n.bytes).sum();
        if rest_bytes > 0 {
            children.push(SpaceNode {
                name: "Các mục khác".into(),
                path: String::new(),
                bytes: rest_bytes,
                is_dir: false,
            });
        }
    }
    acc.dev_artifacts.retain(|item| item.bytes > 0);
    acc.dev_artifacts
        .sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
    acc.dev_artifacts.truncate(TOP_ARTIFACTS);
    SpaceAnalysisReport {
        scanned_path: display_path(root),
        parent_path: root.parent().map(display_path),
        total_bytes,
        scanned_files: acc.scanned_files,
        scanned_dirs: acc.scanned_dirs,
        truncated: acc.truncated,
        children,
        largest_files: std::mem::take(&mut acc.largest_files),
        dev_artifacts: std::mem::take(&mut acc.dev_artifacts),
    }
}

/// Scans `root` recursively (read-only, bounded) for child usage, largest files, and dev artifacts.
///
/// # Errors
/// Returns an error if the directory cannot be read.
pub fn analyze_dir(root: &Path) -> Result<SpaceAnalysisReport, String> {
    let entries = fs::read_dir(root).map_err(|e| format!("Không thể đọc thư mục: {e}"))?;
    let mut acc = ScanAccumulator {
        visited: 0,
        scanned_files: 0,
        scanned_dirs: 0,
        truncated: false,
        largest_files: Vec::new(),
        dev_artifacts: Vec::new(),
    };
    let (mut children, mut total_bytes) = (Vec::new(), 0_u64);
    for entry in entries.flatten() {
        if acc.visited >= MAX_ENTRIES {
            acc.truncated = true;
            break;
        }
        acc.visited += 1;
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_reparse_or_symlink(&meta) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let (bytes, is_dir) = if meta.file_type().is_dir() {
            acc.scanned_dirs += 1;
            (scan_subtree(&path, &name, &mut acc), true)
        } else if meta.file_type().is_file() {
            acc.scanned_files += 1;
            record_large_file(&mut acc.largest_files, &name, &path, meta.len());
            (meta.len(), false)
        } else {
            continue;
        };
        total_bytes = total_bytes.saturating_add(bytes);
        children.push(SpaceNode {
            name,
            path: display_path(&path),
            bytes,
            is_dir,
        });
    }
    Ok(finalize_report(root, &mut acc, children, total_bytes))
}

/// Resolves `target` and runs [`analyze_dir`].
///
/// # Errors
/// Returns target resolution or directory scan errors.
pub fn analyze_space(target: &str) -> Result<SpaceAnalysisReport, String> {
    let path = resolve_target(target)?;
    analyze_dir(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "netdisk-space-{}-{}",
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
    fn analyzes_children_largest_files_and_deduplicates_nested_dev_artifacts() {
        let root = TestDir::new();
        let proj = root.0.join("app");
        let nested_nm = proj.join("node_modules/pkg/node_modules");
        fs::create_dir_all(&nested_nm).unwrap();
        fs::write(proj.join("node_modules/lib.js"), vec![1_u8; 400]).unwrap();
        fs::write(nested_nm.join("deep.js"), vec![2_u8; 600]).unwrap();
        fs::write(root.0.join("big.bin"), vec![3_u8; 2_000]).unwrap();

        let report = analyze_dir(&root.0).unwrap();
        assert_eq!(report.total_bytes, 3_000);
        assert_eq!(report.children.len(), 2);
        assert_eq!(report.children[0].name, "big.bin");
        assert_eq!(report.largest_files[0].name, "big.bin");
        assert_eq!(report.dev_artifacts.len(), 1);
        assert_eq!(report.dev_artifacts[0].name, "node_modules");
        assert_eq!(report.dev_artifacts[0].bytes, 1_000);
    }

    #[test]
    fn rejects_empty_or_non_directory_targets() {
        assert!(resolve_target("").is_err());
        let root = TestDir::new();
        let file = root.0.join("plain.txt");
        fs::write(&file, b"hi").unwrap();
        assert!(resolve_target(file.to_str().unwrap()).is_err());
    }
}
