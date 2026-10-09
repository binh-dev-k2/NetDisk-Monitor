# NetDisk Monitor

Desktop monitor cho Windows và macOS: network upload/download, disk read/write,
tổng lưu lượng trong phiên và danh sách process theo disk I/O.

Bản refactor có collector disk native IOKit trên macOS và network theo PID bằng
ETW trên Windows khi có quyền admin. Kết quả thực nghiệm và giới hạn nằm trong
[hồ sơ kiểm chứng](docs/verification-2026-10-10.md); không đồng nhất việc có code
với việc đã nghiệm thu mọi cấu hình máy.

## Kiến trúc chạy lâu

- Worker tổng máy: 1 giây; worker process: 5 giây, không chặn nhau hoặc UI.
- Tổng phiên lấy từ backend; thời gian process tính từ lần đầu app quan sát.
- Process dùng PID + thời điểm tạo để tránh gộp nhầm khi Windows tái sử dụng PID.
- Taskbar dùng một renderer Win32 riêng, không nhúng WebView vào Explorer.
- Kho binary cố định 1.347.584 byte, gồm journal khôi phục, giữ 7 ngày theo phút và 366 ngày theo ngày.
- Tab tổng quan có lịch sử và CSV; process chỉ giữ trong phiên, tối đa 512 process đã thoát.
- Ghi lịch sử/cài đặt, autostart, kill process và CSV không chạy công việc chậm trên UI.

Không hứa "100%" từ kiểm thử tự động. Phạm vi và giới hạn được ghi trong
[kế hoạch refactor](docs/refactor-24x7.md) và [hồ sơ kiểm chứng](docs/verification-2026-10-10.md).

## Build không cần dev server

    npm run build
    cargo build --release --manifest-path src-tauri/Cargo.toml --features desktop,tauri/custom-protocol

Probe collector thật (không mở giao diện, không chỉnh autostart):

    cargo test --manifest-path src-tauri/Cargo.toml --features desktop --lib collector::tests::live_sampling_probe -- --ignored --nocapture

## Chạy phát triển

Yêu cầu: Node.js 20+, Rust 1.89+, và các toolchain chính thức của Tauri cho
nền tảng đang dùng.

```powershell
npm install
npm run tauri:dev
```

## Kiểm tra

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --features desktop --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --features desktop --all-targets -- -D warnings
```

## Giới hạn theo ứng dụng

I/O theo process được lấy trực tiếp từ `sysinfo`. Windows ETW đo các sự kiện
TCP/UDP IPv4/IPv6 send/receive theo identity native, không phân bổ từ tổng máy.
Thiếu quyền, mất sự kiện hoặc không xác minh được process được báo rõ; không tự
yêu cầu UAC. Network theo PID trên macOS vẫn chưa hỗ trợ. Tổng process là dữ liệu
đã quan sát trong phiên, không phải sổ kế toán toàn bộ vòng đời OS.

I/O process bao gồm hoạt động do hệ điều hành báo, không đồng nhất với I/O vật lý
của ổ đĩa. Tổng network là tổng các giao diện mạng, có thể gồm VPN/adapter ảo.
Phút/ngày là bucket UTC; CSV dùng UTC, giao diện hiển thị theo giờ địa phương.

Taskbar hiện chỉ hỗ trợ thanh ngang chính có khoảng trống đã xác minh giữa cụm
widget bên trái và Start ở giữa. Không đủ chỗ hoặc không xác minh được thì app
báo lỗi và không vẽ đè. Taskbar trái, thanh dọc và màn hình phụ chưa hỗ trợ.
Thanh nổi vẫn có thể bật/tắt, kéo và ghim riêng; độ mờ chỉ áp dụng cho thanh nổi.
macOS dùng chữ trên menu bar, một hàng theo giới hạn hệ điều hành. Overlay trong
suốt bật feature `macos-private-api` của Tauri; đây không phải bản cam kết phù hợp
Mac App Store hoặc đã ký/notarize.

Disk macOS chỉ nhận counter của driver có metadata phân loại hợp lệ; thiếu metadata
thì báo không đo được. CI đã đối chiếu SDK và counter thật trên runner Intel; runner
ARM ảo thiếu `Physical Interconnect`, nên chưa chứng minh đo disk trên Mac ARM thật.

Kho lịch sử không tăng kích thước, nhưng file CSV do người dùng xuất không thuộc
giới hạn này. Mất điện/crash có thể mất batch chưa ghi của phút hiện tại. Checkpoint
phút/ngày dùng redo journal có hai bản checksum; khôi phục after-image thay vì cộng
lại để tránh lệch hai mức hoặc đếm trùng. Dữ liệu v1 được migrate mà không viết
lại record hợp lệ. Độ bền giả định thiết bị tuân thủ flush barrier.

Backend soak có thể chạy bằng `scripts/windows-backend-soak.ps1` (mặc định 24 giờ).
Test này không nghiệm thu UI/WebView, không đổi autostart và không dùng lịch sử thật.

## Nguồn Tham Khảo

[TrafficMonitor Win11 taskbar](https://github.com/zhongyang219/TrafficMonitor/blob/master/TrafficMonitor/Win11TaskbarDlg.cpp)
được dùng để đối chiếu vị trí tương đối với Start và vùng thông báo.
[GetDpiForWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow)
được dùng cho DPI của taskbar; không dùng DPI desktop chung cho nhiều màn hình.
