# Đề xuất tái thiết kế NetDisk Monitor

Ngày khảo sát: 17/11/2026. P0 và P1A đã có bằng chứng local; wireframe đã được duyệt.
Repo: `D:\Codetool\laragon\www\WORK\NetDisk-Monitor`.
Baseline: `6ddf6eb`, nhánh `codex/24x7-native-validation-20261010`.

Kết quả lượt khảo sát:
- [x] Đọc hiện trạng và hồ sơ lỗi/kiểm chứng.
- [x] Tra version stable, tài liệu nền tảng và source đối chứng.
- [x] So sánh kiến trúc, đề xuất UI/luồng và đặt timebox/gate.
- [x] Người dùng duyệt hướng bằng yêu cầu “Làm”; P0 không bao gồm nâng đồng bộ nền tảng.

Chưa kiểm chứng trong lượt này: CPU/RAM, lỗi tự thoát, pixels native, macOS process
network và soak 24h. Không có blocker cho việc lập phương án. Các gate cần Mac thật,
quyền admin hoặc signing credentials được tách riêng, không giả định đã có bằng chứng.

## 1. Kết luận

Không nên viết lại trắng toàn bộ hoặc đổi framework chỉ để làm mới giao diện.
Giữ Rust, Tauri, React và phần đo/lưu trữ đã có; thay ranh giới module, lifecycle,
đường truyền dữ liệu, các presenter native và toàn bộ bố cục/luồng UI theo từng lát cắt.
Phần tương tác Explorer cần được cô lập khỏi core, thay vì tiếp tục vá trong UI.

Ưu tiên: quan sát được lỗi và thoát/mở đúng -> dữ liệu đúng -> chạy nền nhẹ ->
taskbar/overlay đúng -> giao diện mới -> đóng gói, cập nhật và nghiệm thu dài hạn.
Nâng package là một công việc có gate riêng, không phải bằng chứng app đã tối ưu.

## 2. Phạm vi và bằng chứng

Đã đọc README, hồ sơ refactor/kiểm chứng/storage/UI, manifest và lockfile,
service/collector/lifecycle/taskbar, snapshot/subscription, bảng process,
cài đặt/overlay, stylesheet, browser smoke và workflow macOS.
Đây là khảo sát kiến trúc trọng điểm, không phải review từng dòng toàn repo.

Đã tra trực tiếp npm registry, crates.io, Rust stable manifest, Node release index,
tài liệu chính thức và source trọng điểm của bốn app đối chứng.
Không cài dependency, không nâng version, không mở app, không bật dev server,
không chạy lại benchmark/soak hay tuyên bố các test cũ là kết quả mới.

### Hiện trạng đã xác minh từ source

| Điểm | Bằng chứng | Hệ quả |
| --- | --- | --- |
| Collector tổng máy/process/storage đã tách worker | `src-tauri/src/service.rs` | Giữ lại; không làm lại scheduler vô ích |
| Core collector vẫn gắn `AppHandle`, state và emit Tauri | `service.rs` | Cần tách domain khỏi host để kiểm thử/supervise độc lập |
| `get_snapshot` clone và trả cả system/process | `lib.rs:209`, `useMonitorSnapshot.ts` | Bootstrap/fallback hai stream có thể trả payload thừa; không phải poll kép mỗi giây khi event khỏe |
| Tab process ẩn vẫn mount và gọi `useProcessTable` | `App.tsx`, `ProcessTab.tsx` | Vẫn nhóm/sort mỗi khi process thay đổi dù không xem tab |
| Cờ enabled taskbar chỉ phản ánh worker/session | `taskbar.rs`, `set_taskbar_mode` | Chưa đủ phân biệt yêu cầu bật, đã gắn, vẽ lỗi và thực sự nhìn thấy |
| Overlay WebView được tạo ngay khi startup | `lib.rs:458`, setup | Có chi phí resident ngay cả khi không dùng overlay |
| Preferences chỉ có flags/opacity 8 byte | `preferences.rs` | Chưa đủ lưu mode, vị trí, màn hình, cột và lựa chọn adapter |
| Thiếu single-instance plugin trong dependency/builder | `Cargo.toml`, `lib.rs` | Khóa history/ETW không thay thế trải nghiệm single-instance toàn app |
| Worker panic chỉ được tổng hợp khi shutdown/join | `service.rs` | Cần heartbeat và lỗi component trong lúc đang chạy |
| Tổng process group dựa trên record còn giữ | `docs/frontend-ux-acceptance.md` | Dọn PID cũ có thể làm tổng nhóm giảm; cần ledger tổng app riêng |
| History binary đã có giới hạn và journal | `storage.rs`, `docs/storage-v2.md` | Không đổi DB chỉ để mang tiếng hiện đại |
| Browser smoke mock IPC | `scripts/browser-smoke.mjs` | Không chứng minh native taskbar, quyền admin, autostart hoặc app không tự thoát |
| CI hiện chủ yếu là macOS | `.github/workflows/macos-disk-validation.yml` | Cần Windows build/runtime/package gate và native E2E |

Các hiện tượng tự thoát và taskbar không nhìn thấy trong hồ sơ tháng 10 chưa được
coi là đã giải quyết. Không suy diễn nguyên nhân mới từ kết quả khảo sát source.

Môi trường đọc được: Node `26.13.0`, Rust `1.93.1`; Windows registry báo
`DisplayVersion=26H2`, build `26300.9974`. Không dùng chuỗi ProductName cũ để kết
luận Windows 10. Cấu hình taskbar đã được kiểm chứng trước đây không đại diện build này.

## 3. Phiên bản stable đã tra trực tiếp

Các số dưới đây là snapshot ngày khảo sát, không phải cam kết luôn mới nhất.
Phải kiểm tra lại registry khi bắt đầu nâng và đóng băng bộ version đã nghiệm thu.

| Thành phần | Đang dùng/cài trong repo | Latest stable đã tra | Cách xử lý |
| --- | --- | --- | --- |
| Node | 26.13.0 | 26.13.1 LTS Lithium | Pin cùng một patch ở local/CI |
| Rust | 1.93.1; manifest MSRV 1.89 | 1.100.0 | Pin toolchain; nâng MSRV thực tế theo dependency |
| Tauri Rust | 2.11.5 | 2.12.6 | Nâng theo batch host/plugins |
| tauri-build | 2.6.3 | 2.7.5 | Đi cùng batch Tauri |
| Tauri JS API / CLI | 2.11.1 / 2.11.4 | 2.12.5 / 2.12.5 | Không ép mọi crate/plugin có cùng số patch |
| plugin-opener JS | manifest ^2, có lockfile | 2.7.3 | Kiểm tra cặp Rust/JS và quyền |
| React / React DOM | 18.3.1 | 19.3.1 | Nâng cùng types và test render |
| TypeScript | 5.9.3 | 7.0.2 | Major riêng; kiểm tra tool nào dùng compiler API |
| Vite | 6.4.3 | 8.3.8 | Có thay đổi bundler/target; kiểm tra assets trên WebView hai OS |
| plugin-react | 4.7.0 | 6.3.0 | Cặp với Vite 8; xác minh peer dependency |
| Vitest | 2.1.9 | 5.1.0 | Nâng test runner riêng, không sửa assertion để che regression |
| Lucide React | 0.468.0 | 1.74.0 | Kiểm tra tên/export icon trước đổi |
| sysinfo | 0.33.1 | 0.40.4 | Cần Rust >=1.95, API khởi tạo có thay đổi; bọc qua adapter |
| windows | direct 0.57.0; lock còn 0.61.3 | 0.62.2 | Gom binding trực tiếp khi hợp lý; không ép transitive dependency |
| windows-sys | 0.61.2 | 0.61.2 | Đã ở latest stable được tra |
| single-instance | Chưa có | 2.5.5 | Candidate; MSRV 1.90, đăng ký trước plugin khác |

Nguồn version: [npm registry](https://registry.npmjs.org/),
[crates.io](https://crates.io/),
[Rust stable manifest](https://static.rust-lang.org/dist/channel-rust-stable.toml),
[Node release index](https://nodejs.org/dist/index.json).
Manifest semver không phải version thực tế: bảng dùng node_modules/Cargo.lock cho baseline.

React docs hiện ở 19.3. Vite 8 dùng Rolldown/Oxc và đổi browser target mặc định.
TypeScript 7 là compiler native, chưa có compiler API ổn định như các bản trước;
không nâng chung với mọi tool rồi mất thời gian tìm lỗi tương thích.
Tăng tốc build cũng không đồng nghĩa giảm CPU/RAM của app đã đóng gói.

Nguồn: [React versions](https://react.dev/versions),
[Vite support](https://vite.dev/releases), [Vite migration](https://vite.dev/guide/migration.html),
[TypeScript 7](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/),
[sysinfo API/MSRV](https://docs.rs/sysinfo/latest/sysinfo/).

## 4. Học gì từ app hiện có

| Dự án / release đã tra | Source đã xem | Bài học áp dụng |
| --- | --- | --- |
| TrafficMonitor V1.86 | `TrafficMonitor/Win11TaskbarDlg.cpp` | Adapter riêng Windows 11, parent/Start/notify, DPI và tọa độ native; không phải API taskbar cross-platform |
| Stats v3.1.0 | `Kit/module/reader.swift`, `Modules/Net/readers.swift` | Reader có start/pause/stop/sleep/interval; menu bar không phụ thuộc dashboard; process network có dùng nettop |
| System Informer v4.0.26241.138 | `plugins/ExtendedTools/etwmon.c` | ETW có quyền, trạng thái session, lỗi, ownership và cleanup; không chia network theo số kết nối |
| bottom 0.15.0 | `src/collection.rs` | Giữ sysinfo instance lâu dài, refresh theo nhóm dữ liệu, chỉ harvest hạng mục được bật và tách việc ít thường xuyên |

Đây là đối chiếu module cụ thể, không chứng nhận mọi quyết định của các app này
đều phù hợp với NetDisk. Không clone/chạy app đối chứng trong lượt này.
Release metadata và source nhánh mặc định được đọc riêng tại ngày khảo sát;
source nhánh mặc định có thể mới hơn release, không được coi là đúng source của tag đó.
Không chép nguyên code; kiểm tra license từng file trước khi tái sử dụng.
GitHub API không nhận diện SPDX của TrafficMonitor; không tự gán license từ trí nhớ.

Nguồn: [TrafficMonitor](https://github.com/zhongyang219/TrafficMonitor),
[Stats reader](https://github.com/exelban/stats/blob/master/Kit/module/reader.swift),
[Stats network](https://github.com/exelban/stats/blob/master/Modules/Net/readers.swift),
[System Informer ETW](https://github.com/winsiderss/systeminformer/blob/master/plugins/ExtendedTools/etwmon.c),
[bottom collector](https://github.com/ClementTsang/bottom/blob/main/src/collection.rs).

## 5. Ba hướng kiến trúc

| Hướng | Lợi | Đổi lại |
| --- | --- | --- |
| A. Giữ một Tauri host, chỉ chia module | Ít thay đổi, ít IPC, tận dụng code | FFI/shell lỗi vẫn có thể làm mất core; giới hạn fault isolation |
| B. Tauri host gọn + core độc lập framework + presenter/helper rủi ro tách process | Giữ UI hiện có làm nền, không để lỗi Explorer kéo sập đo/lưu | Có protocol, supervision và đóng gói helper phải kiểm chứng |
| C. Đổi toàn bộ sang WinUI/C++ và SwiftUI | Native toàn phần, tránh WebView dashboard | Hai bộ UI/toolchain lớn; tốn công, khó bảo đảm parity và regression |

Đề xuất B, theo bước nhỏ. Không tách mọi worker thành microservice và không thêm
Windows Service/root daemon mặc định. Chi phí helper phải được đo; chưa có số đo
để tuyên bố B nhẹ hơn A trên mọi máy. Không đổi sang Electron hay framework UI mới.

### Ranh giới thành phần

```text
Tauri host: single-instance, tray/menu bar, lifecycle, settings, diagnostics
  -> metrics-core: semantics, counters, session ledger, bounded latest snapshots
      -> Windows/macOS adapters: system, process, disk, network
      -> history-store: một writer, binary rings, journal
  -> UI bridge: snapshot/channel theo nhu cầu, không phát cả process vào mọi view
      -> React dashboard: tạo khi cần, đóng view không dừng collector
  -> native presenters
      -> Windows taskbar helper: process riêng, không WebView, không can thiệp Explorer code
      -> overlay/menu bar: adapter nền tảng; benchmark native overlay trước khi thay WebView
  -> optional Windows privileged collector helper: chỉ khi người dùng bật đo admin
```

- Core không biết AppHandle, DOM, vị trí cửa sổ hoặc chuỗi tiếng Việt.
- Mỗi collector có owner, cadence, cancellation, heartbeat, last-success và lỗi riêng.
- Latest-value slot hoặc channel có dung lượng cố định; frame cũ bị thay, không xếp hàng vô hạn.
- Tách `SystemSnapshot`, `ProcessSnapshot`, `Capabilities`, `DisplayStatus` và history query.
- Giữ sequence/generation để bỏ kết quả cũ khi reset, đổi mode hoặc subscribe lại.
- Dữ liệu UI có nguồn đo, thời điểm quan sát, tình trạng thiếu/quyền/mất sự kiện.
- Dùng channel cho luồng snapshot; events cho thay đổi trạng thái ít thường xuyên.
- Chỉ route đang xem mới subscribe phần chi tiết. Collector cần cho ledger vẫn chạy nền.
- Shared store React nhỏ, snapshot immutable/được cache; cân nhắc useSyncExternalStore,
  không thêm Redux/Zustand/React Compiler hoặc virtualization nếu chưa có bằng chứng cần.

Protocol helper có version, frame limit, handshake và ACL đúng user; không mở localhost
server mặc định, không shell string. Helper chỉ nhận lệnh/payload được whitelist.
UI thường không chạy admin. Helper admin không được nhận yêu cầu chạy command tùy ý.
Restart helper có backoff, tối đa 3 lần/10 phút rồi báo degraded; không vòng lặp tự mở app.
Chỉ kết thúc helper thuộc chính phiên app, không kill Explorer hay trace session của app khác.

Lựa chọn này là suy luận thiết kế từ rủi ro của dự án, không phải yêu cầu bắt buộc của Tauri.
Nguồn: [Tauri process model](https://v2.tauri.app/concept/process-model/),
[IPC channels/events](https://v2.tauri.app/develop/calling-frontend/),
[single instance](https://v2.tauri.app/plugin/single-instance/),
[React external store](https://react.dev/reference/react/useSyncExternalStore).

## 6. Dữ liệu và lưu trữ lâu dài

Giữ history system v2 hiện tại: 1.347.584 byte, 7 ngày theo phút, 366 ngày theo ngày,
một writer, ghi batch, checksum và recovery idempotent. Không ghi từng tick, không
nén từng record nhỏ, không tăng cấp sang SQLite chỉ để lưu vài counter cố định.
Đây là lựa chọn phù hợp yêu cầu hiện tại, không phải chứng minh binary luôn nhỏ/nhanh
nhất. SQLite hợp lý hơn nếu sau này cần truy vấn tùy ý; WAL/checkpoint vẫn cần chính sách.

- Không đổi format history trong cùng đợt nâng UI/framework.
- Ledger tổng theo app tách khỏi danh sách PID chi tiết: dọn PID không làm tổng phiên giảm.
- Ledger có giới hạn app key; nếu hết chỗ phải báo/gộp mục Khác, không âm thầm hứa giữ vô hạn.
- PID identity vẫn gồm thời điểm tạo. App key dài hạn không lấy tên file đơn lẻ.
- Thời gian process bắt đầu lúc app quan sát; thời gian nhóm không cộng chồng các process song song.
- Tổng qua nhiều lần mở app là hạng mục riêng: nếu cần, dùng daily rollup có retention và budget cố định,
  không tự lưu mọi PID, endpoint hay gói mạng suốt đời.
- Optional app history cần chốt số app/ngày giữ và format trước khi làm; không hứa đủ mọi process vô hạn
  nhưng dung lượng cố định. UI/CSV phải phân biệt retention, coverage và tổng đã quan sát.
- Có reader/export versioned và test migration; không âm thầm reset file khi lỗi/khác version.
- Giữ rõ giới hạn mất batch chưa flush khi crash. Logical bytes không phải physical SSD writes.
- Settings schema versioned, atomic replace, debounce ghi; metric visibility/mode/vị trí/cột đều phục hồi.
- Log có giới hạn dung lượng, rotation và rate limit; không log mỗi tick, không KeepAll vô hạn.

Budget đề xuất cho dữ liệu app quản lý: <=16 MiB steady state gồm history, settings,
rollup tùy chọn và log/crash text. CSV xuất thủ công và dữ liệu do OS/WebView/WER quản lý
không nằm trong budget này và phải được ghi riêng khi đo, không giấu khỏi báo cáo tổng.
Binary log/event ring và settings nhỏ cần đơn giản, đọc được bằng công cụ hỗ trợ đi kèm.

Nguồn: `docs/storage-v2.md`, [SQLite WAL](https://sqlite.org/wal.html),
[Tauri log sizing/rotation](https://v2.tauri.app/plugin/logging/).

## 7. Taskbar, overlay và quyền đo

### Windows

Taskbar presenter là compatibility adapter theo shell/build/DPI, không phải universal API.
Không nhúng dashboard/overlay WebView vào Explorer; không inject DLL, không hook sửa shell,
không dời icon hệ thống, không tự thay registry giao diện Windows.

Phân biệt `desired`, `starting`, `attached`, `degraded`, `unsupported`, `disabled`.
Cờ worker sống không được biến thành lời khẳng định người dùng đã nhìn thấy bảng.
Kiểm tra parent/bounds/z-order/paint/heartbeat; ảnh desktop thật vẫn là release gate.
Geometry invalid phải hủy proof cũ. Đổi DPI/display/Explorer cần phục hồi state machine,
không gọi ShowWindow/SetWindowPos/UIA dồn dập trên UI thread.

Event layout được coalesce; watchdog tần suất thấp để phục hồi khi mất event. UIA chỉ
tra khi cần, có timeout/backoff. API shell treo không giữ lock core hoặc làm nút UI đơ.
Không đủ vùng an toàn thì báo rõ, giữ tray hoạt động và cho chọn overlay; không tự vẽ đè.

Taskbar tối đa 2 hàng; số cột không bị giới hạn cứng nhưng vẫn bị giới hạn bởi chỗ trống
thực tế. Người dùng chọn metric và ưu tiên, không thể hứa vô hạn cột trên taskbar hữu hạn.
Slot số/unit ổn định theo DPI, không nhảy width mỗi tick. Theme, high contrast và text scale có test.
Overlay desktop tối đa 2 cột, tự fit nội dung, kéo được, pin/opacity độc lập và lưu vị trí.

Network theo PID: ETW thật, đúng identity và báo loss/permission. Giữ rõ I/O process
khác disk vật lý; tổng adapter có VPN/ảo/loopback không tự bằng tổng byte PID.
Không dùng tỷ lệ connection để chia lưu lượng. Không tự hiện UAC hoặc terminal.

### macOS

Menu bar dùng API/native layout macOS, compact một hàng và popover; không ép hai hàng
như Windows. Giữ native IOKit disk adapter và trạng thái unsupported khi driver thiếu counter.
Overlay native NSPanel là candidate để bỏ nhu cầu transparent WebView/private API;
chỉ quyết định sau spike CPU/RAM, drag, Spaces/fullscreen và đóng gói.

Stats hiện dùng nettop cho process network. Đây là candidate có thể thử, không phải API
đo byte public đã được bảo đảm schema/quyền trên mọi macOS. Spike có timeout, parser fixture,
chi phí CPU, quyền và PID reuse; không spawn utility mỗi giây một cách mù quáng.
Không đủ bằng chứng thì giữ unsupported, không biến dữ liệu thiếu thành 0.
Network Extension/entitlement/signing là một hướng khác có chi phí riêng, không tự thêm
system extension vào bản đầu. Parity network theo PID macOS chưa được cam kết.

Nguồn: [Win32 SetParent/DPI](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setparent),
[ETW session](https://learn.microsoft.com/en-us/windows/win32/etw/configuring-and-starting-an-event-tracing-session),
[NSStatusItem](https://developer.apple.com/documentation/appkit/nsstatusitem),
[NEFilterDataProvider](https://developer.apple.com/documentation/networkextension/nefilterdataprovider).

## 8. Thiết kế giao diện và luồng mới

UI là công cụ giám sát gọn, không landing page, không nhiều card lồng nhau.
Theo theme hệ thống: nền neutral sáng/charcoal, màu dữ liệu cyan/green/amber/violet
chỉ dùng cho đúng tín hiệu; chữ 12-14px, số tabular, spacing nhất quán, icon Lucide.
Không shadow/glow trang trí, không banner lỗi dài nối bằng dấu |, không scale chữ theo viewport.

| Khu vực | Thiết kế / hành vi |
| --- | --- |
| Điều hướng | Tổng quan, Ứng dụng, Lịch sử, Cài đặt; sidebar nhỏ ở cửa sổ rộng, thu thành rail/tab khi hẹp |
| Tổng quan | Một dải số realtime; chart network/disk; tổng phiên và adapter selector; không nhét settings/history dài vào cùng màn hình |
| Ứng dụng | Bảng compact: app/PID, thời gian theo dõi, realtime network/I/O, tổng phiên, RAM; tìm/lọc, chọn cột, sort, expand PID, export |
| Lịch sử | Phút/ngày, thời gian/adapter nếu hỗ trợ, coverage, retention, xuất CSV; empty/loading/error rõ ràng |
| Cài đặt | Taskbar và overlay bật/tắt độc lập, metric/ưu tiên, opacity/pin, autostart, retention và quyền đo |
| Chẩn đoán | Status từng component, tuổi mẫu, nguồn đo, lỗi và retry đúng component; xuất gói hỗ trợ đã bỏ dữ liệu nhạy cảm |

Tên app, PID và số không đẩy lệch hàng. Default chỉ hiện cột cần thiết, detail ở expand/drawer.
Giữ page size để hạn chế DOM; chỉ thêm virtualization nếu benchmark thật yêu cầu.
420/640/800/1024/1440px có bố cục riêng phù hợp. Chỉ bảng rộng mới scroll ngang,
toolbar và nút luôn trong khung. Có keyboard/focus, tooltip, light/dark và giảm chuyển động.

### Luồng chính

1. Mở app: single-instance trước collector; đọc settings; tray hiện sớm; khôi phục yêu cầu hiển thị.
   Autostart có thể background-only, không flash dashboard hoặc mở terminal.
2. Mở dashboard: tạo/lấy WebView khi cần, subscribe trước bootstrap, nhận snapshot đúng topic.
   UI không tự tích phân rate thành tổng hoặc tự chạy timer sửa số session.
3. Đóng dashboard: dừng subscription và giải phóng view theo policy đã đo; collector/tray vẫn chạy.
   Exit phải xuất phát từ ý định Quit rõ ràng, không đồng nhất đóng cửa sổ với thoát app.
4. Bật taskbar/overlay: hiển thị pending, lưu desired state, chờ ack đúng generation; nếu lỗi
   giữ trạng thái thực và lý do, không radio/switch báo bật trong khi renderer mất.
5. Đổi autostart: một thao tác async có read-back và rollback trạng thái khi lỗi; không shell.
6. Dữ liệu thiếu/quyền: hiển thị không khả dụng + lý do một chỗ, không rải stack trace trên mọi hàng.
7. Quit: stop producer, lấy mẫu cuối, flush rồi đóng; có deadline và lỗi component cụ thể.
   Không join vô hạn trên UI; không âm thầm kill core nếu flush lỗi. OS shutdown/crash dùng recovery contract.

## 9. Tiêu chí nghiệm thu đề xuất

Đây là budget mục tiêu, chưa đo đạt trên phiên bản hiện tại; baseline P0 sẽ xác nhận/chỉnh có lý do.
Không đổi budget âm thầm chỉ để test xanh.

| Hạng mục | Gate |
| --- | --- |
| UI responsiveness | Không công việc blocking trên UI; thao tác thông thường p95 <100ms, phản hồi pending <100ms |
| Tray-only CPU | Mục tiêu trung bình <=1% của một logical core khi idle; báo riêng mode ETW/process-heavy |
| RAM | Đo toàn process tree; mục tiêu host/native nền <=50 MiB, presenter thêm <=20 MiB, dashboard tổng <=150 MiB |
| Chạy lâu | So sánh sau warmup giờ 2 và giờ 24; median RAM tăng <=10 MiB và không có xu hướng tăng đều |
| Persistence | History system đúng kích thước cố định; write/error/recovery có kiểm thử; không tăng log/queue vô hạn |
| Lifecycle | 100 lần open/close/mode toggle không leak process/HWND/subscription; single-instance và restore hoạt động |
| Taskbar | Ảnh/video desktop thật; không che Weather/Start/icon/tray, không mất khi focus app khác |
| Dữ liệu | Replay counter reset, PID reuse, sleep/gap, ETW loss, partial permissions, clock rollback; không fake 0 |
| Cross-platform | Windows build thực tế và các build công bố hỗ trợ; macOS Intel/ARM, kiểm thử UI và phần cứng cần thiết |

Kiểm thử thành ba tầng: unit/replay -> app Tauri thật qua WebDriver -> desktop/native
acceptance. Tài liệu Tauri hiện hỗ trợ WebdriverIO embedded trên Windows/macOS/Linux;
tauri-driver trực tiếp vẫn có giới hạn khác. Embedded driver chỉ bật trong test build,
không ship IPC mocking/debug endpoint vào production. Kiểm thử DOM không chứng minh
text taskbar/menu bar đã nằm đúng trên desktop.

CI thêm Windows và macOS, lockfile/toolchain rõ ràng, timeout và artifact/log/screenshot.
Release build thật, không dev server; kiểm tra install/update/uninstall/autostart sau packaging.
Code signing/notarization và signed updater tách gate; thiếu certificate/secret phải báo,
không coi binary CI chưa ký là bản phát hành xong. Không auto update trong lúc dùng nếu chưa chọn policy.

Nguồn: [Tauri WebDriver](https://v2.tauri.app/develop/tests/webdriver/),
[autostart](https://v2.tauri.app/plugin/autostart/),
[signed updater](https://v2.tauri.app/plugin/updater/).

## 10. Lộ trình và quy tắc chống lặp

| Giai đoạn | Sản phẩm bắt buộc | Giới hạn phiên làm việc / điểm dừng |
| --- | --- | --- |
| R. Khảo sát | Tài liệu này, inventory version/source/risk | <=25 phút; không sửa runtime trong giai đoạn này |
| P0. Baseline/lifecycle | Profile bản cũ; exit reason/panic/heartbeat bounded; single-instance; test close vs quit | Lát cắt <=90 phút; chưa rõ tự thoát thì thêm bằng chứng, không đoán sửa |
| P1. Nâng nền | Pin Node/Rust; từng batch Tauri/plugins, collector/bindings, React/types, Vite/Vitest/TS/icon | Mỗi batch <=90 phút; tối đa 1 retry install/build do môi trường |
| P2. Core/data contract | Core không Tauri; snapshot/capabilities/ledger/transport có test | Một vertical slice <=90 phút; giữ public contract cũ qua adapter tới khi thay xong |
| P3. Native presenters | Taskbar helper + actual status; overlay/menu bar spike, không giả có API | Spike mỗi chủ đề <=45 phút; attach/debug một case <=30 phút trước khi ghi rõ blocker |
| P4. UI mới | Mỗi view + một luồng end-to-end, responsive và screenshot | Mỗi lát cắt <=90 phút; tối đa 2 vòng layout cho cùng một case |
| P5. Lưu dữ liệu bổ sung | Chỉ làm app-history khi chốt scope/budget; migration và reader | Không trộn format migration với sửa taskbar hoặc nâng framework |
| P6. Release | Native matrix, packaged build, 24h soak, report đạt/không đạt | Soak một run 24h, timeout 26h; không khởi chạy lại nếu chưa có thay đổi liên quan |

Đây là timebox cho một lát cắt/phiên, không phải hứa toàn app hoàn thành trong số phút đó.
Hết timebox phải giao artifact, tóm tắt bằng chứng, chia nhỏ hoặc đổi hướng; không tiếp tục vô hạn.

### Quy tắc vận hành bắt buộc

1. Mỗi việc có câu hỏi, giả thuyết, phép đo nhỏ nhất và tiêu chí xong trước khi chạy tool.
2. Ghi issue ledger: ID, build/OS, bước tái hiện, evidence, thay đổi, kết quả, bước tiếp theo.
   Không đọc lại toàn repo hoặc research lại chủ đề đã có nguồn nếu không có mâu thuẫn mới.
3. Một bug tối đa 2 giả thuyết hoặc 30 phút chưa có bằng chứng mới thì dừng sửa, tổng hợp lại.
   Ba patch thất bại cùng một triệu chứng bắt buộc xem lại thiết kế, không patch thứ tư theo cảm tính.
4. Một lệnh lỗi môi trường: một fallback không cài đặt; sau đó ghi blocker. Không xóa cache,
   tạo worktree/node_modules/store hoặc reinstall lặp đi lặp lại để tìm may mắn.
5. Một thay đổi -> test nhỏ liên quan; full suite/build chạy một lần khi candidate ổn định.
   Chạy lại chỉ khi code/config/evidence liên quan đổi, không chạy để có cảm giác chắc hơn.
6. Không mở thêm instance khi chưa kiểm tra instance hiện có. Không thay bản đang chạy,
   restart Explorer, đổi quyền/registry/taskbar setting trong lúc chỉ nghiên cứu.
7. Taskbar không có chỗ/API chưa rõ thì báo unsupported/degraded và tiếp tục core/UI;
   không bịa tọa độ, cờ visible hoặc mock API để vượt gate.
8. Mỗi slice có diff nhỏ, checkpoint và rollback rõ. Không merge release gate đỏ;
   cũng không coi bản cũ không đọc được format mới là rollback dữ liệu hợp lệ.
9. Báo tiến độ theo artifact, không theo số lệnh hay số giờ. Kết thúc mỗi phiên ghi Done,
   Unverified, Blocked, Next; không hứa 100% từ test tự động.
10. Trước mỗi release re-query stable/advisory và đóng băng lockfile; không dùng beta/canary
    chỉ để được gọi là mới nhất. Nếu version mới không qua gate, giữ bản đã chứng minh và ghi lý do.

## 11. Checklist quyết định/hiểu biết trước khi code

- [ ] Phân biệt nâng version với tối ưu CPU/RAM và sửa lifecycle.
- [ ] Hiểu thread tách riêng không tự tạo fault isolation như process riêng.
- [ ] Chấp nhận taskbar có capability/compatibility matrix, không hứa mọi Windows shell.
- [ ] Phân biệt byte adapter, byte PID, I/O process và disk vật lý.
- [ ] Chốt giữ process trong phiên hay cần app-history qua restart, cùng retention cụ thể.
- [x] Chốt hướng B và bố cục bốn view.
- [x] Duyệt wireframe trước khi làm UI production (người dùng duyệt bố cục bốn view ngày 17/11/2026).
- [ ] Chấp nhận các gate native/24h và phần chưa chứng minh trên Mac thật.

Chỉ các quyết định được người dùng xác nhận mới được đánh dấu; các mục hiểu biết
còn lại chưa được xác minh. Bố cục không đồng nghĩa với nghiệm thu native.

## 12. Trạng thái triển khai P0

P0 đã thêm chính sách Quit rõ ràng, khóa chủ sở hữu host, log có giới hạn và heartbeat
worker. Kiểm thử Rust cuối: 151 đạt, 8 bỏ qua có chủ đích; Clippy nghiêm ngặt và
rustfmt đạt. Prototype bốn view đã được duyệt; chưa thay UI production.

Chi tiết thay đổi, bằng chứng, giới hạn và các gate tiếp theo nằm trong
[hồ sơ P0](p0-evidence-2026-11-17.md). Chưa đo baseline CPU/RAM trên app thật,
chưa nghiệm thu taskbar/lifecycle native, chưa chạy soak 24 giờ cho bản thay đổi này.

P1A đã nâng đồng bộ cohort Tauri Rust/JS và bổ sung CI Windows/macOS. Test/lint/build
Windows local đạt, chưa chạy CI và chưa thay UI production. Xem [hồ sơ P1A](p1a-evidence-2026-11-17.md)
và [điểm đích/checklist release](DELIVERY.md) trước khi bắt đầu lát cắt tiếp theo.
