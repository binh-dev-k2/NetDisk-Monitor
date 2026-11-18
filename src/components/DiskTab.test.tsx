import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { SmartCleanerView, type SmartCleanerViewProps } from './disk/SmartCleanerView';
import { SpaceAnalyzerView, type SpaceAnalyzerViewProps } from './disk/SpaceAnalyzerView';
import type { CleanupScanReport, SpaceAnalysisReport, VolumeUsage } from '../types';

const sampleVolumes: VolumeUsage[] = [{
  mountPoint: 'C:\\', name: 'OS', fileSystem: 'NTFS', totalBytes: 500_000_000_000, availableBytes: 150_000_000_000, usedBytes: 350_000_000_000, isRemovable: false,
}];

const sampleSpaceReport: SpaceAnalysisReport = {
  scannedPath: 'C:\\Users\\Test\\Downloads',
  parentPath: 'C:\\Users\\Test',
  totalBytes: 943_718_400,
  scannedFiles: 120,
  scannedDirs: 18,
  truncated: false,
  children: [
    { name: 'Installers', path: 'C:\\Users\\Test\\Downloads\\Installers', bytes: 629_145_600, isDir: true },
    { name: 'Projects', path: 'C:\\Users\\Test\\Downloads\\Projects', bytes: 314_572_800, isDir: true },
  ],
  largestFiles: [{ name: 'sdk.iso', path: 'C:\\Users\\Test\\Downloads\\Installers\\sdk.iso', bytes: 629_145_600 }],
  devArtifacts: [{ name: 'node_modules', path: 'C:\\Users\\Test\\Downloads\\Projects\\demo\\node_modules', bytes: 314_572_800 }],
};

const sampleCleanReport: CleanupScanReport = {
  totalReclaimableBytes: 52_428_800,
  totalFiles: 12,
  rules: [
    { id: 'temp_files', category: 'Hệ thống', label: 'Tệp tạm hệ thống (> 24 giờ)', description: 'Tệp tạm cũ', path: 'C:\\Temp', bytes: 52_428_800, fileCount: 12, exists: true, safetyNote: 'Giữ tệp < 24h' },
    { id: 'npm_cache', category: 'Developer', label: 'npm Package Cache', description: 'Gói npm cache', path: 'C:\\npm-cache', bytes: 0, fileCount: 0, exists: false, safetyNote: 'Tự tải lại' },
  ],
};

const renderSpace = (overrides: Partial<SpaceAnalyzerViewProps> = {}) => renderToStaticMarkup(
  <SpaceAnalyzerView volumes={sampleVolumes} target="downloads" report={sampleSpaceReport} subtab="files" loading={false} error={null} notice={null} onSelectTarget={vi.fn()} onRescan={vi.fn()} onSelectSubtab={vi.fn()} onRevealPath={vi.fn()} {...overrides} />,
);

const renderClean = (overrides: Partial<SmartCleanerViewProps> = {}) => renderToStaticMarkup(
  <SmartCleanerView report={sampleCleanReport} selected={new Set(['temp_files'])} loading={false} cleaning={false} confirming={false} error={null} notice={null} onRescan={vi.fn()} onToggleRule={vi.fn()} onRequestClean={vi.fn()} onConfirmClean={vi.fn()} onCancelClean={vi.fn()} onRevealPath={vi.fn()} {...overrides} />,
);

describe('DiskTab views', () => {
  it('renders SpaceAnalyzerView loading, error, empty, treemap and artifacts states', () => {
    expect(renderSpace({ report: null, loading: true })).toContain('Đang phân tích cấu trúc dung lượng thư mục...');
    const failed = renderSpace({ report: null, error: 'Không thể đọc thư mục' });
    expect(failed).toContain('role="alert"');
    expect(failed).toContain('Thử lại');
    expect(renderSpace({ report: { ...sampleSpaceReport, children: [] } })).toContain('Thư mục trống hoặc không có tệp nào đo được.');
    const filesHtml = renderSpace();
    expect(filesHtml).toContain('aria-label="Mở thư mục Installers: 600.0 MB"');
    expect(filesHtml).toContain('sdk.iso');
    expect(renderSpace({ subtab: 'artifacts' })).toContain('node_modules');
  });

  it('renders SmartCleanerView 4 UI states and 2-step destructive confirmation', () => {
    expect(renderClean({ report: null, loading: true })).toContain('Đang quét các thư mục tạm và bộ đệm...');
    const failed = renderClean({ report: null, error: 'Lỗi quét bộ đệm' });
    expect(failed).toContain('role="alert"');
    expect(failed).toContain('Thử lại');
    expect(renderClean({ report: { rules: [], totalReclaimableBytes: 0, totalFiles: 0 } })).toContain('Không có quy tắc dọn dẹp khả dụng.');
    const ready = renderClean();
    expect(ready).toContain('Tệp tạm hệ thống');
    expect(ready).toContain('Dọn mục đã chọn');
    const confirmHtml = renderClean({ confirming: true });
    expect(confirmHtml).toContain('Xóa trực tiếp không qua Thùng rác');
    expect(confirmHtml).toContain('Xác nhận dọn ngay');
    expect(confirmHtml).toContain('Hủy');
  });
});
