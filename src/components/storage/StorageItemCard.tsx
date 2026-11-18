import { formatBytes } from '../../lib/format';
import type { ClearTarget, StorageItem } from '../../types';

type StorageItemCardProps = {
  item: StorageItem;
  busy: boolean;
  confirmHistory: boolean;
  onRequestClear: (target: ClearTarget) => void;
  onConfirmHistory: () => void;
  onCancelConfirm: () => void;
};

export function StorageItemCard({
  item, busy, confirmHistory, onRequestClear, onConfirmHistory, onCancelConfirm,
}: StorageItemCardProps) {
  return (
    <div className="worker-card storage-card">
      <div className="worker-card-top">
        <strong>{item.label}</strong>
        <span className={`status-badge ${item.clearable ? 'muted' : 'ok'}`}>
          {item.maxBytes !== null ? `${formatBytes(item.bytes)} / ${formatBytes(item.maxBytes)}` : formatBytes(item.bytes)}
        </span>
      </div>
      {item.maxBytes !== null && item.maxBytes > 0 && (
        <div className="storage-meter-track" aria-hidden="true">
          <div className="storage-meter-fill" style={{ inlineSize: `${Math.min(100, Math.round((item.bytes / item.maxBytes) * 100))}%` }} />
        </div>
      )}
      <small className="storage-detail">{item.detail}</small>
      <div className="worker-card-meta">
        <span>{item.recordCount !== null ? `${item.recordCount} mục/bản ghi` : 'Do hệ thống quản lý'}</span>
        <span>{item.clearable ? 'Có thể dọn' : 'Giữ cố định'}</span>
      </div>
      <div className="storage-card-actions">
        {item.id === 'diagnostics' && (
          <button type="button" className="inline-action-btn" disabled={busy} onClick={() => onRequestClear('diagnostics')}>
            Xóa nhật ký
          </button>
        )}
        {item.id === 'history' && (confirmHistory ? (
          <>
            <button type="button" className="inline-action-btn danger" disabled={busy} onClick={onConfirmHistory}>Xác nhận xóa lịch sử</button>
            <button type="button" className="inline-action-btn" disabled={busy} onClick={onCancelConfirm}>Hủy</button>
          </>
        ) : (
          <button type="button" className="inline-action-btn" disabled={busy} onClick={() => onRequestClear('history')}>Xóa lịch sử</button>
        ))}
      </div>
    </div>
  );
}
