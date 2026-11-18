#[cfg(all(not(windows), feature = "desktop"))]
use std::collections::HashMap;
#[cfg(any(all(windows, feature = "desktop"), test))]
use std::collections::{HashMap, VecDeque};

#[cfg(any(feature = "desktop", test))]
pub(crate) const MAX_ICON_CACHE: usize = 512;

#[cfg(feature = "desktop")]
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIconQuery {
    pub name: String,
    pub pid: String,
}

#[cfg(any(feature = "desktop", test))]
pub(crate) struct IconCache {
    #[cfg(any(all(windows, feature = "desktop"), test))]
    capacity: usize,
    #[cfg(any(all(windows, feature = "desktop"), test))]
    by_name: HashMap<String, Option<String>>,
    #[cfg(any(all(windows, feature = "desktop"), test))]
    name_order: VecDeque<String>,
    #[cfg(any(all(windows, feature = "desktop"), test))]
    by_path: HashMap<String, Option<String>>,
    #[cfg(any(all(windows, feature = "desktop"), test))]
    path_order: VecDeque<String>,
}

#[cfg(any(feature = "desktop", test))]
impl Default for IconCache {
    fn default() -> Self {
        Self::new(MAX_ICON_CACHE)
    }
}

#[cfg(any(feature = "desktop", test))]
impl IconCache {
    #[must_use]
    pub(crate) fn new(capacity: usize) -> Self {
        #[cfg(not(any(all(windows, feature = "desktop"), test)))]
        let _ = capacity;
        Self {
            #[cfg(any(all(windows, feature = "desktop"), test))]
            capacity: capacity.max(1),
            #[cfg(any(all(windows, feature = "desktop"), test))]
            by_name: HashMap::new(),
            #[cfg(any(all(windows, feature = "desktop"), test))]
            name_order: VecDeque::new(),
            #[cfg(any(all(windows, feature = "desktop"), test))]
            by_path: HashMap::new(),
            #[cfg(any(all(windows, feature = "desktop"), test))]
            path_order: VecDeque::new(),
        }
    }

    #[cfg(any(all(windows, feature = "desktop"), test))]
    fn normalize_key(value: &str) -> String {
        value.to_ascii_lowercase()
    }

    #[cfg(any(all(windows, feature = "desktop"), test))]
    fn insert_bounded(
        map: &mut HashMap<String, Option<String>>,
        order: &mut VecDeque<String>,
        capacity: usize,
        key: String,
        value: Option<String>,
    ) {
        if let Some(existing) = map.get_mut(&key) {
            *existing = value;
            return;
        }
        while map.len() >= capacity {
            let Some(oldest) = order.pop_front() else {
                break;
            };
            map.remove(&oldest);
        }
        order.push_back(key.clone());
        map.insert(key, value);
    }

    #[cfg(any(all(windows, feature = "desktop"), test))]
    pub(crate) fn get_name(&self, name: &str) -> Option<&Option<String>> {
        self.by_name.get(&Self::normalize_key(name))
    }

    #[cfg(any(all(windows, feature = "desktop"), test))]
    pub(crate) fn insert_name(&mut self, name: &str, value: Option<String>) {
        Self::insert_bounded(
            &mut self.by_name,
            &mut self.name_order,
            self.capacity,
            Self::normalize_key(name),
            value,
        );
    }

    #[cfg(any(all(windows, feature = "desktop"), test))]
    pub(crate) fn get_path(&self, path: &str) -> Option<&Option<String>> {
        self.by_path.get(&Self::normalize_key(path))
    }

    #[cfg(any(all(windows, feature = "desktop"), test))]
    pub(crate) fn insert_path(&mut self, path: &str, value: Option<String>) {
        Self::insert_bounded(
            &mut self.by_path,
            &mut self.path_order,
            self.capacity,
            Self::normalize_key(path),
            value,
        );
    }

    pub(crate) fn cached_count(&self) -> usize {
        #[cfg(any(all(windows, feature = "desktop"), test))]
        {
            self.by_name.len().max(self.by_path.len())
        }
        #[cfg(not(any(all(windows, feature = "desktop"), test)))]
        {
            0
        }
    }

    pub(crate) fn clear(&mut self) {
        #[cfg(any(all(windows, feature = "desktop"), test))]
        {
            self.by_name.clear();
            self.name_order.clear();
            self.by_path.clear();
            self.path_order.clear();
        }
    }
}

#[cfg(any(all(windows, feature = "desktop"), test))]
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(any(all(windows, feature = "desktop"), test))]
fn adler32(bytes: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut s1 = 1_u32;
    let mut s2 = 0_u32;
    for &byte in bytes {
        s1 = (s1 + u32::from(byte)) % MOD_ADLER;
        s2 = (s2 + s1) % MOD_ADLER;
    }
    (s2 << 16) | s1
}

#[cfg(any(all(windows, feature = "desktop"), test))]
fn push_png_chunk(out: &mut Vec<u8>, tag: [u8; 4], data: &[u8]) {
    let len = u32::try_from(data.len()).unwrap_or(0);
    out.extend_from_slice(&len.to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&tag);
    out.extend_from_slice(data);
    let checksum = crc32(&out[start..]);
    out.extend_from_slice(&checksum.to_be_bytes());
}

#[cfg(any(all(windows, feature = "desktop"), test))]
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = if chunk.len() > 1 {
            u32::from(chunk[1])
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            u32::from(chunk[2])
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(any(all(windows, feature = "desktop"), test))]
pub(crate) fn encode_rgba_png_data_uri(width: u32, height: u32, rgba: &[u8]) -> Option<String> {
    if !(1..=64).contains(&width) || !(1..=64).contains(&height) {
        return None;
    }
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if rgba.len() != expected {
        return None;
    }
    let row_bytes = (width as usize) * 4;
    let raw_len = (height as usize) * (1 + row_bytes);
    let block_len = u16::try_from(raw_len).ok()?;
    let nlen = !block_len;

    let mut raw = Vec::with_capacity(raw_len);
    for y in 0..height as usize {
        raw.push(0);
        let row_start = y * row_bytes;
        raw.extend_from_slice(&rgba[row_start..row_start + row_bytes]);
    }

    let mut zlib = Vec::with_capacity(2 + 5 + raw.len() + 4);
    zlib.extend_from_slice(&[0x78, 0x01, 0x01]);
    zlib.extend_from_slice(&block_len.to_le_bytes());
    zlib.extend_from_slice(&nlen.to_le_bytes());
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut png = Vec::with_capacity(8 + 25 + 12 + zlib.len() + 12);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    push_png_chunk(&mut png, *b"IHDR", &ihdr);
    push_png_chunk(&mut png, *b"IDAT", &zlib);
    push_png_chunk(&mut png, *b"IEND", &[]);

    let mut uri = String::from("data:image/png;base64,");
    uri.push_str(&base64_encode(&png));
    Some(uri)
}

#[cfg(any(all(windows, feature = "desktop"), test))]
pub(crate) fn bgra_to_rgba(color_bgra: &[u8], mask_bgra: Option<&[u8]>) -> Vec<u8> {
    let has_alpha = color_bgra.chunks_exact(4).any(|px| px[3] != 0);
    let mut rgba = Vec::with_capacity(color_bgra.len());
    for (index, px) in color_bgra.chunks_exact(4).enumerate() {
        let alpha = if has_alpha {
            px[3]
        } else if let Some(mask) = mask_bgra {
            let offset = index * 4;
            mask.get(offset)
                .map_or(255, |&b| if b == 0 { 255 } else { 0 })
        } else {
            255
        };
        rgba.extend_from_slice(&[px[2], px[1], px[0], alpha]);
    }
    rgba
}

#[cfg(any(all(windows, feature = "desktop"), test))]
pub(crate) fn downsample_rgba_max_32(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> (u32, u32, Vec<u8>) {
    if width <= 32 && height <= 32 {
        return (width, height, rgba);
    }
    let dst_w = 32_u32.min(width);
    let dst_h = 32_u32.min(height);
    let cap = usize::try_from(dst_w).unwrap_or(32) * usize::try_from(dst_h).unwrap_or(32) * 4;
    let width_usize = usize::try_from(width).unwrap_or(32);
    let mut out = Vec::with_capacity(cap);
    for y in 0..dst_h {
        let src_y =
            usize::try_from((u64::from(y) * u64::from(height)) / u64::from(dst_h)).unwrap_or(0);
        for x in 0..dst_w {
            let src_x =
                usize::try_from((u64::from(x) * u64::from(width)) / u64::from(dst_w)).unwrap_or(0);
            let idx = (src_y * width_usize + src_x) * 4;
            if let Some(pixel) = rgba.get(idx..idx + 4) {
                out.extend_from_slice(pixel);
            } else {
                out.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    (dst_w, dst_h, out)
}

#[cfg(all(windows, feature = "desktop"))]
mod native {
    use super::{
        bgra_to_rgba, downsample_rgba_max_32, encode_rgba_png_data_uri, IconCache, ProcessIconQuery,
    };
    use std::{
        collections::HashMap,
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
        ptr::null_mut,
        sync::Mutex,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
        },
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
    };

    #[link(name = "shell32")]
    extern "system" {
        fn ExtractIconExW(
            lpsz_file: *const u16,
            icon_index: i32,
            phicon_large: *mut HICON,
            phicon_small: *mut HICON,
            n_icons: u32,
        ) -> u32;
    }

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    struct IconGuard(HICON);
    impl Drop for IconGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DestroyIcon(self.0);
                }
            }
        }
    }

    struct BitmapGuard(HBITMAP);
    impl Drop for BitmapGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DeleteObject(self.0.cast());
                }
            }
        }
    }

    struct DcGuard(HDC);
    impl Drop for DcGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DeleteDC(self.0);
                }
            }
        }
    }

    fn query_exe_path(pid: u32) -> Option<String> {
        if pid == 0 {
            return None;
        }
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if raw.is_null() {
            return None;
        }
        let _guard = HandleGuard(raw);
        let mut buf = [0_u16; 1024];
        let mut len = u32::try_from(buf.len()).unwrap_or(1024);
        if unsafe { QueryFullProcessImageNameW(raw, 0, buf.as_mut_ptr(), &raw mut len) } == 0
            || len == 0
        {
            return None;
        }
        let os_str = OsString::from_wide(&buf[..len as usize]);
        let path = os_str.to_string_lossy().into_owned();
        (!path.is_empty()).then_some(path)
    }

    fn read_dib_bgra(dc: HDC, bmp: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
        let len = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?
            .checked_mul(4)?;
        let mut buf = vec![0_u8; len];
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).ok()?,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let scanlines = unsafe {
            GetDIBits(
                dc,
                bmp,
                0,
                u32::try_from(height).ok()?,
                buf.as_mut_ptr().cast(),
                &raw mut bmi,
                DIB_RGB_COLORS,
            )
        };
        (scanlines != 0).then_some(buf)
    }

    pub(super) fn extract_exe_icon_data_uri(path: &str) -> Option<String> {
        if path.starts_with(r"\\") {
            return None;
        }
        let wide: Vec<u16> = std::ffi::OsStr::new(path)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut large: HICON = null_mut();
        let mut small: HICON = null_mut();
        let count = unsafe { ExtractIconExW(wide.as_ptr(), 0, &raw mut large, &raw mut small, 1) };
        let large_guard = IconGuard(large);
        let small_guard = IconGuard(small);
        if count == 0 || count == u32::MAX {
            return None;
        }
        let chosen = if !large_guard.0.is_null() {
            large_guard.0
        } else if !small_guard.0.is_null() {
            small_guard.0
        } else {
            return None;
        };

        let mut info = ICONINFO::default();
        if unsafe { GetIconInfo(chosen, &raw mut info) } == 0 {
            return None;
        }
        let color_bmp = BitmapGuard(info.hbmColor);
        let mask_bmp = BitmapGuard(info.hbmMask);
        if color_bmp.0.is_null() {
            return None;
        }

        let mut bmp = BITMAP::default();
        let bmp_size = i32::try_from(std::mem::size_of::<BITMAP>()).ok()?;
        if unsafe { GetObjectW(color_bmp.0.cast(), bmp_size, (&raw mut bmp).cast()) } == 0 {
            return None;
        }
        if !(1..=256).contains(&bmp.bmWidth) || !(1..=256).contains(&bmp.bmHeight) {
            return None;
        }

        let dc = DcGuard(unsafe { CreateCompatibleDC(null_mut()) });
        if dc.0.is_null() {
            return None;
        }
        let color_bgra = read_dib_bgra(dc.0, color_bmp.0, bmp.bmWidth, bmp.bmHeight)?;
        let has_alpha = color_bgra.chunks_exact(4).any(|px| px[3] != 0);
        let mask_bgra = if !has_alpha && !mask_bmp.0.is_null() {
            read_dib_bgra(dc.0, mask_bmp.0, bmp.bmWidth, bmp.bmHeight)
        } else {
            None
        };
        let rgba = bgra_to_rgba(&color_bgra, mask_bgra.as_deref());
        let width_u32 = u32::try_from(bmp.bmWidth).ok()?;
        let height_u32 = u32::try_from(bmp.bmHeight).ok()?;
        let (final_w, final_h, final_rgba) = downsample_rgba_max_32(width_u32, height_u32, rgba);
        encode_rgba_png_data_uri(final_w, final_h, &final_rgba)
    }

    pub(crate) fn resolve_batch(
        cache_mutex: &Mutex<IconCache>,
        queries: Vec<ProcessIconQuery>,
    ) -> HashMap<String, Option<String>> {
        let mut results = HashMap::new();
        let mut misses = Vec::new();
        {
            let Ok(cache) = cache_mutex.lock() else {
                return results;
            };
            for query in queries.into_iter().take(64) {
                if let Some(hit) = cache.get_name(&query.name) {
                    results.insert(query.name, hit.clone());
                } else {
                    misses.push(query);
                }
            }
        }
        if misses.is_empty() {
            return results;
        }

        let mut resolved_entries: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        let mut local_paths: HashMap<String, Option<String>> = HashMap::new();
        for query in misses {
            let pid = query.pid.parse::<u32>().ok().unwrap_or(0);
            let Some(path) = query_exe_path(pid) else {
                results.entry(query.name).or_insert(None);
                continue;
            };
            let path_key = path.to_ascii_lowercase();
            let icon = if let Some(cached_in_batch) = local_paths.get(&path_key) {
                cached_in_batch.clone()
            } else {
                let cached_in_global = cache_mutex
                    .lock()
                    .ok()
                    .and_then(|cache| cache.get_path(&path_key).cloned());
                let resolved = cached_in_global.unwrap_or_else(|| extract_exe_icon_data_uri(&path));
                local_paths.insert(path_key, resolved.clone());
                resolved
            };
            results.insert(query.name.clone(), icon.clone());
            resolved_entries.push((query.name, Some(path), icon));
        }

        if let Ok(mut cache) = cache_mutex.lock() {
            for (name, path_opt, icon) in resolved_entries {
                if let Some(path) = path_opt {
                    cache.insert_path(&path, icon.clone());
                    cache.insert_name(&name, icon);
                }
            }
        }
        results
    }
}

#[cfg(all(windows, feature = "desktop"))]
pub(crate) fn resolve_batch(
    cache_mutex: &std::sync::Mutex<IconCache>,
    queries: Vec<ProcessIconQuery>,
) -> HashMap<String, Option<String>> {
    native::resolve_batch(cache_mutex, queries)
}

#[cfg(all(not(windows), feature = "desktop"))]
pub(crate) fn resolve_batch(
    _cache_mutex: &std::sync::Mutex<IconCache>,
    _queries: Vec<ProcessIconQuery>,
) -> HashMap<String, Option<String>> {
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::{bgra_to_rgba, downsample_rgba_max_32, encode_rgba_png_data_uri, IconCache};

    #[test]
    fn png_encoder_produces_valid_ihdr_idat_iend_and_data_uri() {
        let uri = encode_rgba_png_data_uri(1, 1, &[255, 0, 128, 255]).unwrap();
        assert!(uri.starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert!(encode_rgba_png_data_uri(0, 16, &[]).is_none());
        assert!(encode_rgba_png_data_uri(65, 1, &[0; 65 * 4]).is_none());
        assert!(encode_rgba_png_data_uri(2, 2, &[0; 4]).is_none());
    }

    #[test]
    fn bgra_conversion_preserves_alpha_and_falls_back_to_and_mask() {
        let with_alpha = bgra_to_rgba(&[10, 20, 30, 200, 1, 2, 3, 0], None);
        assert_eq!(with_alpha, vec![30, 20, 10, 200, 3, 2, 1, 0]);

        let mask_fallback = bgra_to_rgba(
            &[10, 20, 30, 0, 40, 50, 60, 0],
            Some(&[0, 0, 0, 0, 255, 255, 255, 0]),
        );
        assert_eq!(mask_fallback, vec![30, 20, 10, 255, 60, 50, 40, 0]);

        let (w, h, downsampled) = downsample_rgba_max_32(64, 64, vec![42; 64 * 64 * 4]);
        assert_eq!((w, h), (32, 32));
        assert_eq!(downsampled.len(), 32 * 32 * 4);
    }

    #[test]
    fn icon_cache_is_bounded_and_evicts_oldest_entries() {
        let mut cache = IconCache::new(2);
        cache.insert_name("Alpha.exe", Some("data:image/png;base64,a".into()));
        cache.insert_name("Beta.exe", None);
        assert!(cache.get_name("alpha.exe").is_some());
        cache.insert_name("Gamma.exe", Some("data:image/png;base64,g".into()));
        assert!(cache.get_name("alpha.exe").is_none());
        assert!(cache.get_name("beta.exe").is_some());
        assert!(cache.get_name("gamma.exe").is_some());

        cache.insert_path(r"C:\Apps\One.exe", Some("data:image/png;base64,1".into()));
        cache.insert_path(r"C:\Apps\Two.exe", None);
        assert!(cache.get_path(r"c:\apps\one.exe").is_some());
        cache.insert_path(r"C:\Apps\Three.exe", Some("data:image/png;base64,3".into()));
        assert!(cache.get_path(r"c:\apps\one.exe").is_none());
        assert!(cache.get_path(r"c:\apps\two.exe").is_some());
        assert!(cache.get_path(r"c:\apps\three.exe").is_some());
        assert_eq!(cache.cached_count(), 2);
        cache.clear();
        assert_eq!(cache.cached_count(), 0);
        assert!(cache.get_name("beta.exe").is_none());
        assert!(cache.get_path(r"c:\apps\two.exe").is_none());
    }

    #[cfg(all(windows, feature = "desktop"))]
    #[test]
    fn extracts_explorer_icon_when_present() {
        let path = r"C:\Windows\explorer.exe";
        if std::path::Path::new(path).exists() {
            if let Some(uri) = super::native::extract_exe_icon_data_uri(path) {
                assert!(uri.starts_with("data:image/png;base64,iVBORw0KGgo"));
            }
        }
    }
}
