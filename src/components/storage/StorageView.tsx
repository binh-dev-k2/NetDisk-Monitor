import { FolderOpen, HardDrive, RefreshCw, Trash2 } from 'lucide-react';
import { formatBytes } from '../../lib/format';
import type { ClearTarget, StorageOverview } from '../../types';
import { StorageItemCard } from './StorageItemCard';

export type StorageViewProps = {
  overview: StorageOverview | null;
  loading: boolean;
  error: string | null;
  notice: string | null;
  pendingAction: string | null;
  confirmTarget: 'history' | 'all_clearable' | null;
  onRefresh: () => void;
  onOpenFolder: (target: 'app_data' | 'webview') => void;
  onRequestClear: (target: ClearTarget) => void;
  onConfirmClear: (target: 'history' | 'all_clearable') => void;
  onCancelConfirm: () => void;
};

export function StorageView(props: StorageViewProps) {
  const { overview, loading, error, notice, pendingAction, confirmTarget } = props;
  const busy = loading || pendingAction !== null;

  return (
    <section className="diagnostics-panel storage-panel" aria-label="Quản lý lưu trữ dữ liệu">
      <div className="diagnostics-header">
        <div>
          <h3>Lưu trữ &amp; Dữ liệu ứng dụng</h3>
          <p className="panel-subtitle">
            {overview
              ? `Dữ liệu app: ${formatBytes(overview.totalAppManagedBytes)} / trần ${formatBytes(overview.maxAppManagedBytes)} · Tự xoay vòng, không gây đầy bộ nhớ`
              : 'Quét vị trí lưu tệp, kiểm tra dung lượng và dọn dữ liệu cũ an toàn'}
          </p>
        </div>
        <div className="storage-header-actions">
          <button type="button" className="icon-button-text" onClick={props.onRefresh} disabled={busy} aria-label="Quét lại lưu trữ">
            <RefreshCw size={13} aria-hidden="true" /> Quét lại
          </button>
          <button type="button" className="icon-button-text" onClick={() => props.onOpenFolder('app_data')} disabled={busy || !overview} aria-label="Mở thư mục dữ liệu">
            <FolderOpen size={13} aria-hidden="true" /> Mở thư mục dữ liệu
          </button>
          {overview?.webViewDataDir && (
            <button type="button" className="icon-button-text" onClick={() => props.onOpenFolder('webview')} disabled={busy} aria-label="Mở thư mục WebView">
              <HardDrive size={13} aria-hidden="true" /> Mở thư mục WebView
            </button>
          )}
        </div>
      </div>

      {error && (
        <div className="diagnostics diagnostics-row" role="alert">
          <span>{error}</span>
          <button type="button" className="inline-action-btn" onClick={props.onRefresh} disabled={busy}>Thử lại</button>
        </div>
      )}
      {notice && <div className="notice" role="status">{notice}</div>}
      {loading && !overview && !error && <div className="empty">Đang quét dung lượng lưu trữ...</div>}
      {!loading && !error && (!overview || overview.items.length === 0) && (
        <div className="empty diagnostics-row">
          <span>Không tìm thấy mục lưu trữ nào.</span>
          <button type="button" className="inline-action-btn" onClick={props.onRefresh}>Quét lại</button>
        </div>
      )}

      {overview && overview.items.length > 0 && (
        <>
          <div className="storage-paths">
            <div className="storage-path-row"><span>Thư mục dữ liệu chính:</span><code>{overview.appDataDir}</code></div>
            {overview.webViewDataDir && <div className="storage-path-row"><span>Thư mục cache WebView2:</span><code>{overview.webViewDataDir}</code></div>}
          </div>
          <div className="worker-grid storage-grid">
            {overview.items.map(item => (
              <StorageItemCard
                key={item.id}
                item={item}
                busy={busy}
                confirmHistory={confirmTarget === 'history'}
                onRequestClear={props.onRequestClear}
                onConfirmHistory={() => props.onConfirmClear('history')}
                onCancelConfirm={props.onCancelConfirm}
              />
            ))}
            <div className="worker-card storage-card">
              <div className="worker-card-top">
                <strong>Bộ nhớ đệm RAM (phiên hiện tại)</strong>
                <span className="status-badge muted">Giới hạn 512 mục</span>
              </div>
              <small className="storage-detail">
                Icon ứng dụng: {overview.memory.iconCacheCount} / {overview.memory.iconCacheLimit} · Tiến trình đã thoát: {overview.memory.terminatedProcessCount} / {overview.memory.terminatedProcessLimit}
              </small>
              <div className="worker-card-actions">
                <button type="button" className="inline-action-btn" disabled={busy} onClick={() => props.onRequestClear('caches')}>
                  Dọn cache RAM
                </button>
              </div>
            </div>
          </div>
          <div className="storage-footer">
            <small className="storage-detail">Giữ nguyên cấu hình thanh nổi, giao diện và tự khởi động trong preferences-v2.bin.</small>
            <div className="storage-card-actions">
              {confirmTarget === 'all_clearable' ? (
                <>
                  <button type="button" className="inline-action-btn danger" disabled={busy} onClick={() => props.onConfirmClear('all_clearable')}>
                    Xác nhận dọn tất cả
                  </button>
                  <button type="button" className="inline-action-btn" disabled={busy} onClick={props.onCancelConfirm}>Hủy</button>
                </>
              ) : (
                <button type="button" className="reset-button compact" disabled={busy} onClick={() => props.onRequestClear('all_clearable')}>
                  <Trash2 size={13} aria-hidden="true" /> Dọn toàn bộ dữ liệu cũ
                </button>
              )}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
