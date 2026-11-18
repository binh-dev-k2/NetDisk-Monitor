import { ArrowUpLeft, FolderOpen, RefreshCw } from 'lucide-react';
import { formatBytes } from '../../lib/format';
import { compactTreemapNodes, layoutTreemap } from '../../lib/treemap';
import type { SpaceAnalysisReport, VolumeUsage } from '../../types';
import { SpaceDetailsList } from './SpaceDetailsList';

export type SpaceAnalyzerViewProps = {
  volumes: VolumeUsage[];
  target: string;
  report: SpaceAnalysisReport | null;
  subtab: 'files' | 'artifacts';
  loading: boolean;
  error: string | null;
  notice: string | null;
  onSelectTarget: (target: string) => void;
  onRescan: () => void;
  onSelectSubtab: (tab: 'files' | 'artifacts') => void;
  onRevealPath: (path: string) => void;
};

const QUICK_TARGETS: ReadonlyArray<{ id: string; label: string }> = [
  { id: 'downloads', label: 'Downloads' },
  { id: 'documents', label: 'Documents' },
  { id: 'desktop', label: 'Desktop' },
  { id: 'local_appdata', label: 'AppData Local' },
  { id: 'home', label: 'Thư mục người dùng' },
];

export function SpaceAnalyzerView(props: SpaceAnalyzerViewProps) {
  const { volumes, target, report, subtab, loading, error, notice } = props;
  const tiles = report ? layoutTreemap(compactTreemapNodes(report.children), 100, 100) : [];

  return (
    <section className="diagnostics-panel space-analyzer-panel" aria-label="Phân tích dung lượng thư mục">
      <div className="diagnostics-header">
        <div>
          <h3>Phân tích dung lượng &amp; Bản đồ đĩa</h3>
          <p className="panel-subtitle">
            {report
              ? `Tổng quét ${formatBytes(report.totalBytes)} · ${report.scannedFiles} tệp, ${report.scannedDirs} thư mục${report.truncated ? ' (giới hạn an toàn 40,000 mục)' : ''}`
              : 'Quét chỉ đọc theo phân vùng hoặc thư mục, tìm tệp lớn và thư mục build dư thừa'}
          </p>
        </div>
        <div className="storage-header-actions">
          <button type="button" className="icon-button-text" onClick={props.onRescan} disabled={loading} aria-label="Quét lại không gian đĩa">
            <RefreshCw size={13} aria-hidden="true" /> Quét lại
          </button>
          {report?.parentPath && (
            <button type="button" className="icon-button-text" onClick={() => props.onSelectTarget(report.parentPath!)} disabled={loading} aria-label="Lên thư mục cha">
              <ArrowUpLeft size={13} aria-hidden="true" /> Lên thư mục cha
            </button>
          )}
          {report && (
            <button type="button" className="icon-button-text" onClick={() => props.onRevealPath(report.scannedPath)} disabled={loading} aria-label="Mở thư mục đang xem">
              <FolderOpen size={13} aria-hidden="true" /> Mở thư mục này
            </button>
          )}
        </div>
      </div>

      <div className="disk-target-pills" role="group" aria-label="Chọn vùng quét nhanh">
        {volumes.map(vol => (
          <button type="button" key={vol.mountPoint} className={`disk-target-btn${target === vol.mountPoint ? ' active' : ''}`} disabled={loading} onClick={() => props.onSelectTarget(vol.mountPoint)}>
            Ổ {vol.mountPoint} ({formatBytes(vol.usedBytes)})
          </button>
        ))}
        {QUICK_TARGETS.map(item => (
          <button type="button" key={item.id} className={`disk-target-btn${target === item.id ? ' active' : ''}`} disabled={loading} onClick={() => props.onSelectTarget(item.id)}>
            {item.label}
          </button>
        ))}
      </div>

      {error && <div className="diagnostics diagnostics-row" role="alert"><span>{error}</span><button type="button" className="inline-action-btn" onClick={props.onRescan} disabled={loading}>Thử lại</button></div>}
      {notice && <div className="notice" role="status">{notice}</div>}
      {loading && !report && !error && <div className="empty">Đang phân tích cấu trúc dung lượng thư mục...</div>}
      {!loading && !error && (!report || report.children.length === 0) && (
        <div className="empty diagnostics-row"><span>Thư mục trống hoặc không có tệp nào đo được.</span><button type="button" className="inline-action-btn" onClick={props.onRescan}>Quét lại</button></div>
      )}

      {report && report.children.length > 0 && (
        <>
          <div className="disk-path-bar"><code>{report.scannedPath}</code><span className="status-badge muted">{report.children.length} nhánh</span></div>
          <div className="treemap-canvas" role="region" aria-label="Bản đồ phân bổ dung lượng">
            {tiles.map((tile, index) => {
              const interactive = Boolean(tile.node.path);
              const toneClass = interactive ? `tone-${index % 5}` : 'is-other';
              return (
                <button
                  type="button"
                  key={`${tile.node.name}-${tile.node.path}`}
                  className={`treemap-tile ${toneClass}`}
                  style={{ left: `${tile.x}%`, top: `${tile.y}%`, width: `${tile.w}%`, height: `${tile.h}%` }}
                  disabled={loading || !interactive}
                  title={`${tile.node.name} (${formatBytes(tile.node.bytes)} · ${tile.sharePercent.toFixed(1)}%)`}
                  aria-label={`${tile.node.isDir ? 'Mở thư mục' : 'Mở vị trí tệp'} ${tile.node.name}: ${formatBytes(tile.node.bytes)}`}
                  onClick={() => tile.node.isDir ? props.onSelectTarget(tile.node.path) : props.onRevealPath(tile.node.path)}
                >
                  {tile.w >= 11 && tile.h >= 16 && <strong>{tile.node.name}</strong>}
                  {tile.w >= 14 && tile.h >= 24 && <span>{formatBytes(tile.node.bytes)}</span>}
                </button>
              );
            })}
          </div>
          <div className="disk-subtabs">
            <button type="button" className={`disk-subtab-btn${subtab === 'files' ? ' active' : ''}`} onClick={() => props.onSelectSubtab('files')}>
              Tệp lớn nhất ({report.largestFiles.length})
            </button>
            <button type="button" className={`disk-subtab-btn${subtab === 'artifacts' ? ' active' : ''}`} onClick={() => props.onSelectSubtab('artifacts')}>
              Thư mục Build / Dev ({report.devArtifacts.length})
            </button>
          </div>
          <SpaceDetailsList subtab={subtab} largestFiles={report.largestFiles} devArtifacts={report.devArtifacts} busy={loading} onReveal={props.onRevealPath} />
        </>
      )}
    </section>
  );
}
