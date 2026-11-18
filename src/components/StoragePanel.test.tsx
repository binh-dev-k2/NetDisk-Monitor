import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { StorageView, type StorageViewProps } from './storage/StorageView';
import type { StorageOverview } from '../types';

const sampleOverview: StorageOverview = {
  appDataDir: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor',
  webViewDataDir: 'C:\\Users\\Test\\AppData\\Local\\com.binhc.netdiskmonitor',
  totalAppManagedBytes: 1_360_000,
  maxAppManagedBytes: 1_876_032,
  totalWebViewBytes: 12_582_912,
  items: [
    {
      id: 'history',
      label: 'Lịch sử lưu lượng (vòng đệm)',
      path: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor\\history-v1.bin',
      bytes: 1_347_584,
      maxBytes: 1_347_584,
      recordCount: 150,
      clearable: true,
      detail: '120 bản ghi phút (tối đa 10080), 30 bản ghi ngày (tối đa 366)',
    },
    {
      id: 'diagnostics',
      label: 'Nhật ký chẩn đoán (xoay vòng)',
      path: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor\\diagnostics',
      bytes: 12_392,
      maxBytes: 528_384,
      recordCount: 2,
      clearable: true,
      detail: 'Gồm events.jsonl, events.1.jsonl và last-fatal.txt',
    },
    {
      id: 'preferences',
      label: 'Cấu hình người dùng & khóa phiên',
      path: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor\\preferences-v2.bin',
      bytes: 24,
      maxBytes: 64,
      recordCount: 2,
      clearable: false,
      detail: 'Lưu vị trí thanh nổi, giao diện, taskbar',
    },
    {
      id: 'webview',
      label: 'Bộ nhớ đệm giao diện WebView2',
      path: 'C:\\Users\\Test\\AppData\\Local\\com.binhc.netdiskmonitor',
      bytes: 12_582_912,
      maxBytes: null,
      recordCount: 48,
      clearable: false,
      detail: 'Cache trình duyệt nội bộ do hệ điều hành quản lý',
    },
  ],
  memory: {
    iconCacheCount: 19,
    iconCacheLimit: 512,
    terminatedProcessCount: 7,
    terminatedProcessLimit: 512,
  },
};

function render(overrides: Partial<StorageViewProps> = {}) {
  return renderToStaticMarkup(
    <StorageView
      overview={sampleOverview}
      loading={false}
      error={null}
      notice={null}
      pendingAction={null}
      confirmTarget={null}
      onRefresh={vi.fn()}
      onOpenFolder={vi.fn()}
      onRequestClear={vi.fn()}
      onConfirmClear={vi.fn()}
      onCancelConfirm={vi.fn()}
      {...overrides}
    />,
  );
}

describe('StorageView four UI states and safe cleanup confirmations', () => {
  it('renders loading state when initial scan is running', () => {
    const html = render({ overview: null, loading: true });
    expect(html).toContain('Đang quét dung lượng lưu trữ...');
  });

  it('renders error state with retry action', () => {
    const html = render({ overview: null, error: 'Không đọc được thư mục dữ liệu' });
    expect(html).toContain('role="alert"');
    expect(html).toContain('Không đọc được thư mục dữ liệu');
    expect(html).toContain('Thử lại');
  });

  it('renders empty state when no storage items exist', () => {
    const html = render({ overview: { ...sampleOverview, items: [] } });
    expect(html).toContain('Không tìm thấy mục lưu trữ nào.');
    expect(html).toContain('Quét lại');
  });

  it('renders exact storage paths, WebView folder button, RAM caches and confirmation buttons', () => {
    const base = render({ notice: 'Đã dọn sạch nhật ký chẩn đoán.' });
    expect(base).toContain('C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor');
    expect(base).toContain('C:\\Users\\Test\\AppData\\Local\\com.binhc.netdiskmonitor');
    expect(base).toContain('Mở thư mục WebView');
    expect(base).toContain('Icon ứng dụng: 19 / 512');
    expect(base).toContain('Tiến trình đã thoát: 7 / 512');
    expect(base).toContain('role="status"');
    expect(base).toContain('Xóa nhật ký');
    expect(base).toContain('Xóa lịch sử');
    expect(base).toContain('Dọn cache RAM');
    expect(base).toContain('Dọn toàn bộ dữ liệu cũ');

    const confirmHistory = render({ confirmTarget: 'history' });
    expect(confirmHistory).toContain('Xác nhận xóa lịch sử');
    expect(confirmHistory).toContain('Hủy');

    const confirmAll = render({ confirmTarget: 'all_clearable' });
    expect(confirmAll).toContain('Xác nhận dọn tất cả');
    expect(confirmAll).toContain('Hủy');
  });
});
