import { FolderOpen, RefreshCw, Trash2 } from 'lucide-react';
import { formatBytes } from '../../lib/format';
import type { CleanupRuleFinding, CleanupRuleId, CleanupScanReport } from '../../types';

export type SmartCleanerViewProps = {
  report: CleanupScanReport | null;
  selected: ReadonlySet<CleanupRuleId>;
  loading: boolean;
  cleaning: boolean;
  confirming: boolean;
  error: string | null;
  notice: string | null;
  onRescan: () => void;
  onToggleRule: (id: CleanupRuleId) => void;
  onRequestClean: () => void;
  onConfirmClean: () => void;
  onCancelClean: () => void;
  onRevealPath: (path: string) => void;
};

type RuleCardProps = {
  rule: CleanupRuleFinding;
  checked: boolean;
  busy: boolean;
  onToggle: (id: CleanupRuleId) => void;
  onReveal: (path: string) => void;
};

function CleanerRuleCard({ rule, checked, busy, onToggle, onReveal }: RuleCardProps) {
  const selectable = rule.exists && (rule.bytes > 0 || rule.fileCount > 0);
  return (
    <article className="worker-card storage-card cleaner-rule-card">
      <div className="worker-card-top">
        <label className="cleaner-rule-check">
          <input type="checkbox" checked={checked} disabled={busy || !selectable} onChange={() => onToggle(rule.id)} aria-label={`Chọn dọn ${rule.label}`} />
          <strong>{rule.label}</strong>
        </label>
        <span className={`status-badge ${selectable ? 'ok' : 'muted'}`}>{rule.exists ? formatBytes(rule.bytes) : 'Không có'}</span>
      </div>
      <p className="storage-detail">{rule.description}</p>
      <div className="storage-footer">
        <div className="storage-card-actions">
          <span className="status-badge muted">{rule.category}</span>
          <span className="storage-detail">{rule.fileCount} tệp · {rule.safetyNote}</span>
        </div>
        {rule.exists && (
          <button type="button" className="inline-action-btn" onClick={() => onReveal(rule.path)} disabled={busy} aria-label={`Mở vị trí ${rule.label}`}>
            <FolderOpen size={12} aria-hidden="true" /> Mở
          </button>
        )}
      </div>
    </article>
  );
}

type FooterProps = Pick<SmartCleanerViewProps, 'confirming' | 'cleaning' | 'onRequestClean' | 'onConfirmClean' | 'onCancelClean'> & {
  busy: boolean;
  selectedCount: number;
  selectedBytes: number;
  selectedFiles: number;
};

function CleanerFooter({ busy, confirming, cleaning, selectedCount, selectedBytes, selectedFiles, onRequestClean, onConfirmClean, onCancelClean }: FooterProps) {
  return (
    <div className="storage-footer" role="region" aria-label="Xác nhận dọn rác">
      <span className="storage-detail">
        {confirming
          ? 'Xóa trực tiếp không qua Thùng rác; tệp đang mở sẽ tự động được bỏ qua an toàn'
          : `Đã chọn ${selectedCount} mục (${formatBytes(selectedBytes)} · ${selectedFiles} tệp)`}
      </span>
      <div className="storage-card-actions">
        {confirming ? (
          <>
            <button type="button" className="reset-button compact" onClick={onConfirmClean} disabled={busy || selectedCount === 0}>
              {cleaning ? 'Đang dọn...' : 'Xác nhận dọn ngay'}
            </button>
            <button type="button" className="inline-action-btn" onClick={onCancelClean} disabled={busy}>Hủy</button>
          </>
        ) : (
          <button type="button" className="reset-button compact" onClick={onRequestClean} disabled={busy || selectedCount === 0}>
            <Trash2 size={13} aria-hidden="true" /> Dọn mục đã chọn
          </button>
        )}
      </div>
    </div>
  );
}

export function SmartCleanerView(props: SmartCleanerViewProps) {
  const { report, selected, loading, cleaning, confirming, error, notice } = props;
  const busy = loading || cleaning;
  const activeRules = report?.rules.filter(r => selected.has(r.id) && r.exists && (r.bytes > 0 || r.fileCount > 0)) ?? [];
  const selectedBytes = activeRules.reduce((sum, r) => sum + r.bytes, 0);
  const selectedFiles = activeRules.reduce((sum, r) => sum + r.fileCount, 0);

  return (
    <section className="diagnostics-panel smart-cleaner-panel" aria-label="Dọn dẹp bộ đệm & rác an toàn">
      <div className="diagnostics-header">
        <div>
          <h3>Dọn rác thông minh (An toàn theo quy tắc)</h3>
          <p className="panel-subtitle">
            {report
              ? `Có thể thu hồi ${formatBytes(report.totalReclaimableBytes)} · ${report.totalFiles} tệp trong các thư mục đệm an toàn`
              : 'Chỉ quét bộ đệm tạm đã kiểm chứng; tuyệt đối không chạm dữ liệu cá nhân'}
          </p>
        </div>
        <div className="storage-header-actions">
          <button type="button" className="icon-button-text" onClick={props.onRescan} disabled={busy} aria-label="Quét lại bộ đệm rác">
            <RefreshCw size={13} aria-hidden="true" /> Quét lại
          </button>
        </div>
      </div>

      {error && <div className="diagnostics diagnostics-row" role="alert"><span>{error}</span><button type="button" className="inline-action-btn" onClick={props.onRescan} disabled={busy}>Thử lại</button></div>}
      {notice && <div className="notice" role="status">{notice}</div>}
      {loading && !report && !error && <div className="empty">Đang quét các thư mục tạm và bộ đệm...</div>}
      {!loading && !error && (!report || report.rules.length === 0) && (
        <div className="empty diagnostics-row"><span>Không có quy tắc dọn dẹp khả dụng.</span><button type="button" className="inline-action-btn" onClick={props.onRescan}>Quét lại</button></div>
      )}

      {report && report.rules.length > 0 && (
        <>
          <div className="worker-grid storage-grid">
            {report.rules.map(rule => (
              <CleanerRuleCard key={rule.id} rule={rule} checked={selected.has(rule.id)} busy={busy} onToggle={props.onToggleRule} onReveal={props.onRevealPath} />
            ))}
          </div>
          <CleanerFooter busy={busy} confirming={confirming} cleaning={cleaning} selectedCount={activeRules.length} selectedBytes={selectedBytes} selectedFiles={selectedFiles} onRequestClean={props.onRequestClean} onConfirmClean={props.onConfirmClean} onCancelClean={props.onCancelClean} />
        </>
      )}
    </section>
  );
}
