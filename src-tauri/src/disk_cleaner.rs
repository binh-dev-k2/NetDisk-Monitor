//! Safety-first deep cleaner backed by curated, read-only-first system & developer cache rules.
use crate::storage_info::is_reparse_or_symlink;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const RULE_MAX_ENTRIES: usize = 16_384;
const RULE_MAX_DEPTH: usize = 8;
const TEMP_MIN_AGE: Duration = Duration::from_secs(86_400);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupRuleId {
    TempFiles,
    CrashDumps,
    WerReports,
    DirectxShaderCache,
    NpmCache,
    PnpmCache,
    PipCache,
    CargoRegistryCache,
    NugetHttpCache,
}

const ALL_RULES: [CleanupRuleId; 9] = [
    CleanupRuleId::TempFiles,
    CleanupRuleId::CrashDumps,
    CleanupRuleId::WerReports,
    CleanupRuleId::DirectxShaderCache,
    CleanupRuleId::NpmCache,
    CleanupRuleId::PnpmCache,
    CleanupRuleId::PipCache,
    CleanupRuleId::CargoRegistryCache,
    CleanupRuleId::NugetHttpCache,
];

struct RuleMeta {
    category: &'static str,
    label: &'static str,
    description: &'static str,
    safety_note: &'static str,
    min_age: Option<Duration>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRuleFinding {
    pub id: CleanupRuleId,
    pub category: String,
    pub label: String,
    pub description: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: usize,
    pub exists: bool,
    pub safety_note: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupScanReport {
    pub rules: Vec<CleanupRuleFinding>,
    pub total_reclaimable_bytes: u64,
    pub total_files: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupExecutionReport {
    pub freed_bytes: u64,
    pub deleted_files: usize,
    pub skipped_locked_files: usize,
    pub refreshed: CleanupScanReport,
}

const fn meta(
    category: &'static str,
    label: &'static str,
    description: &'static str,
    safety_note: &'static str,
    min_age: Option<Duration>,
) -> RuleMeta {
    RuleMeta {
        category,
        label,
        description,
        safety_note,
        min_age,
    }
}

fn rule_meta(id: CleanupRuleId) -> RuleMeta {
    match id {
        CleanupRuleId::TempFiles => meta(
            "Hệ thống",
            "Tệp tạm hệ thống (> 24 giờ)",
            "Các tệp tạm cài đặt và bộ đệm đã cũ trên 24 giờ trong thư mục Temp",
            "Giữ lại toàn bộ tệp mới tạo trong 24h gần nhất",
            Some(TEMP_MIN_AGE),
        ),
        CleanupRuleId::CrashDumps => meta(
            "Hệ thống",
            "Bản ghi lỗi CrashDumps",
            "Tệp dump bộ nhớ ứng dụng sau sự cố không còn cần cho vận hành thường nhật",
            "Không ảnh hưởng ứng dụng đang chạy",
            None,
        ),
        CleanupRuleId::WerReports => meta(
            "Hệ thống",
            "Báo cáo lỗi Windows (WER)",
            "Kho lưu báo cáo sự cố Windows Error Reporting đã gửi xong",
            "An toàn xóa hoàn toàn",
            None,
        ),
        CleanupRuleId::DirectxShaderCache => meta(
            "Hệ thống",
            "Bộ đệm Shader DirectX",
            "Cache biên dịch shader cũ của GPU; tự tạo lại khi mở game hoặc app đồ họa",
            "Tệp đang được GPU khóa sẽ tự động bỏ qua",
            None,
        ),
        _ => dev_rule_meta(id),
    }
}

fn dev_rule_meta(id: CleanupRuleId) -> RuleMeta {
    match id {
        CleanupRuleId::NpmCache => meta(
            "Developer",
            "npm Package Cache",
            "Gói nén npm đã tải trước; không ảnh hưởng tới node_modules hiện có",
            "Chỉ xóa cache tải về, tái tạo tự động khi cài gói",
            None,
        ),
        CleanupRuleId::PnpmCache => meta(
            "Developer",
            "pnpm Metadata Cache",
            "Cache HTTP/metadata của pnpm (không chạm tới store hardlink)",
            "Giữ nguyên toàn bộ gói trong store dùng chung",
            None,
        ),
        CleanupRuleId::PipCache => meta(
            "Developer",
            "Python pip Cache",
            "Bộ nhớ đệm wheel và gói tải về của pip",
            "Không ảnh hưởng môi trường ảo hoặc thư viện đã cài",
            None,
        ),
        CleanupRuleId::CargoRegistryCache => meta(
            "Developer",
            "Rust Cargo Registry (.crate)",
            "Bản nén .crate đã tải trước trong CARGO_HOME/registry/cache",
            "Giữ nguyên src đã giải nén và binary đã cài",
            None,
        ),
        _ => meta(
            "Developer",
            ".NET NuGet v3 Cache",
            "Bộ nhớ đệm HTTP tạm khi khôi phục gói NuGet",
            "Không ảnh hưởng gói đã khôi phục trong dự án",
            None,
        ),
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn resolve_rule_path(id: CleanupRuleId) -> Option<PathBuf> {
    let local_app = env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let home = home_dir();
    match id {
        CleanupRuleId::TempFiles => env::var_os("TEMP")
            .or_else(|| env::var_os("TMP"))
            .map(PathBuf::from)
            .or_else(|| local_app.as_ref().map(|p| p.join("Temp"))),
        CleanupRuleId::CrashDumps => local_app.map(|p| p.join("CrashDumps")),
        CleanupRuleId::WerReports => {
            local_app.map(|p| p.join("Microsoft/Windows/WER/ReportArchive"))
        }
        CleanupRuleId::DirectxShaderCache => local_app.map(|p| p.join("D3DSCache")),
        CleanupRuleId::NpmCache => env::var_os("NPM_CONFIG_CACHE")
            .map(PathBuf::from)
            .or_else(|| local_app.map(|p| p.join("npm-cache")))
            .or_else(|| home.map(|p| p.join(".npm"))),
        CleanupRuleId::PnpmCache => local_app
            .map(|p| p.join("pnpm-cache"))
            .or_else(|| home.map(|p| p.join(".cache/pnpm"))),
        CleanupRuleId::PipCache => local_app
            .map(|p| p.join("pip/cache"))
            .or_else(|| home.map(|p| p.join(".cache/pip"))),
        CleanupRuleId::CargoRegistryCache => env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|p| p.join(".cargo")))
            .map(|p| p.join("registry/cache")),
        CleanupRuleId::NugetHttpCache => env::var_os("NUGET_HTTP_CACHE_PATH")
            .map(PathBuf::from)
            .or_else(|| local_app.map(|p| p.join("NuGet/v3-cache"))),
    }
}

fn is_safe_cleanup_root(path: &Path) -> bool {
    let min_components = if cfg!(windows) { 4 } else { 3 };
    if path.components().count() < min_components {
        return false;
    }
    fs::symlink_metadata(path)
        .ok()
        .is_some_and(|meta| meta.file_type().is_dir() && !is_reparse_or_symlink(&meta))
}

fn is_eligible_age(meta: &fs::Metadata, min_age: Option<Duration>, now: SystemTime) -> bool {
    let Some(min_age) = min_age else {
        return true;
    };
    meta.modified()
        .ok()
        .and_then(|mtime| now.duration_since(mtime).ok())
        .is_some_and(|age| age >= min_age)
}

fn collect_rule_targets(
    root: &Path,
    min_age: Option<Duration>,
    now: SystemTime,
) -> (Vec<(PathBuf, u64)>, Vec<PathBuf>) {
    if !is_safe_cleanup_root(root) {
        return (Vec::new(), Vec::new());
    }
    let mut stack = vec![(root.to_path_buf(), 0_usize)];
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    let mut visited = 0_usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if visited >= RULE_MAX_ENTRIES {
                return (files, subdirs);
            }
            visited += 1;
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_file() && is_eligible_age(&meta, min_age, now) {
                files.push((path, meta.len()));
            } else if meta.file_type().is_dir()
                && !is_reparse_or_symlink(&meta)
                && depth < RULE_MAX_DEPTH
            {
                subdirs.push(path.clone());
                stack.push((path, depth + 1));
            }
        }
    }
    (files, subdirs)
}

fn clean_rule_at(root: &Path, min_age: Option<Duration>, now: SystemTime) -> (u64, usize, usize) {
    let (files, subdirs) = collect_rule_targets(root, min_age, now);
    let (mut freed_bytes, mut deleted_files, mut skipped_locked) = (0_u64, 0_usize, 0_usize);
    for (file, size) in files {
        match fs::remove_file(&file) {
            Ok(()) => {
                freed_bytes = freed_bytes.saturating_add(size);
                deleted_files += 1;
            }
            Err(_) => skipped_locked += 1,
        }
    }
    for dir in subdirs.into_iter().rev() {
        let _ = fs::remove_dir(dir);
    }
    (freed_bytes, deleted_files, skipped_locked)
}

#[must_use]
pub fn scan_rules() -> CleanupScanReport {
    let now = SystemTime::now();
    let mut rules = Vec::with_capacity(ALL_RULES.len());
    let (mut total_reclaimable_bytes, mut total_files) = (0_u64, 0_usize);
    for id in ALL_RULES {
        let meta = rule_meta(id);
        let path_opt = resolve_rule_path(id);
        let safe_root = path_opt.as_deref().filter(|p| is_safe_cleanup_root(p));
        let exists = safe_root.is_some();
        let (bytes, file_count) = safe_root.map_or((0, 0), |root| {
            let (files, _) = collect_rule_targets(root, meta.min_age, now);
            let total = files.iter().map(|(_, size)| *size).sum();
            (total, files.len())
        });
        total_reclaimable_bytes = total_reclaimable_bytes.saturating_add(bytes);
        total_files = total_files.saturating_add(file_count);
        rules.push(CleanupRuleFinding {
            id,
            category: meta.category.into(),
            label: meta.label.into(),
            description: meta.description.into(),
            path: path_opt.map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
            bytes,
            file_count,
            exists,
            safety_note: meta.safety_note.into(),
        });
    }
    CleanupScanReport {
        rules,
        total_reclaimable_bytes,
        total_files,
    }
}

#[must_use]
pub fn execute_rules(selected: &[CleanupRuleId]) -> CleanupExecutionReport {
    let now = SystemTime::now();
    let chosen: HashSet<CleanupRuleId> = selected.iter().copied().collect();
    let (mut freed_bytes, mut deleted_files, mut skipped_locked_files) = (0_u64, 0_usize, 0_usize);
    for id in ALL_RULES {
        if !chosen.contains(&id) {
            continue;
        }
        let Some(root) = resolve_rule_path(id).filter(|p| is_safe_cleanup_root(p)) else {
            continue;
        };
        let (freed, deleted, skipped) = clean_rule_at(&root, rule_meta(id).min_age, now);
        freed_bytes = freed_bytes.saturating_add(freed);
        deleted_files = deleted_files.saturating_add(deleted);
        skipped_locked_files = skipped_locked_files.saturating_add(skipped);
    }
    CleanupExecutionReport {
        freed_bytes,
        deleted_files,
        skipped_locked_files,
        refreshed: scan_rules(),
    }
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
                "netdisk-cleaner-{}-{}",
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
    fn rejects_shallow_system_roots_and_preserves_fresh_temp_files() {
        assert!(!is_safe_cleanup_root(Path::new("/")));
        #[cfg(windows)]
        {
            assert!(!is_safe_cleanup_root(Path::new(r"C:\")));
            assert!(!is_safe_cleanup_root(Path::new(r"C:\Users")));
        }

        let dir = TestDir::new();
        let nested = dir.0.join("sub");
        fs::create_dir_all(&nested).unwrap();
        let fresh = nested.join("recent.tmp");
        fs::write(&fresh, vec![1_u8; 512]).unwrap();

        let (kept_files, _) = collect_rule_targets(&dir.0, Some(TEMP_MIN_AGE), SystemTime::now());
        assert!(kept_files.is_empty());

        let future = SystemTime::now() + TEMP_MIN_AGE + Duration::from_secs(60);
        let (freed, deleted, skipped) = clean_rule_at(&dir.0, Some(TEMP_MIN_AGE), future);
        assert_eq!((freed, deleted, skipped), (512, 1, 0));
        assert!(!fresh.exists());
        assert!(dir.0.exists());
    }
}
