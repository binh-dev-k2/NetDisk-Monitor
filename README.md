# NetDisk Monitor

Desktop monitor cho Windows và macOS: network upload/download, disk read/write,
tổng lưu lượng trong phiên và danh sách process theo disk I/O.

## Chạy phát triển

Yêu cầu: Node.js 20+, Rust stable, và các toolchain chính thức của Tauri cho
nền tảng đang dùng.

```powershell
npm install
npm run tauri:dev
```

## Kiểm tra

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

## Giới hạn theo ứng dụng

Disk I/O theo process được lấy trực tiếp từ `sysinfo`. Network per-process chưa
được hiển thị byte count vì Windows và macOS cần collector native khác nhau để
có dữ liệu chính xác. UI gắn rõ trạng thái này thay vì phân bổ giả từ tổng
network của máy.
