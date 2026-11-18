import { FolderOpen } from 'lucide-react';
import { formatBytes } from '../../lib/format';
import type { DevArtifactEntry, LargeFileEntry } from '../../types';

type SpaceDetailsListProps = {
  subtab: 'files' | 'artifacts';
  largestFiles: LargeFileEntry[];
  devArtifacts: DevArtifactEntry[];
  busy: boolean;
  onReveal: (path: string) => void;
};

export function SpaceDetailsList({ subtab, largestFiles, devArtifacts, busy, onReveal }: SpaceDetailsListProps) {
  const entries = subtab === 'files' ? largestFiles : devArtifacts;
  if (!entries.length) {
    return (
      <div className="empty">
        {subtab === 'files'
          ? 'Không tìm thấy tệp nào có dung lượng đáng kể.'
          : 'Không phát hiện thư mục build/cache developer nào trong phạm vi quét.'}
      </div>
    );
  }
  return (
    <div className="disk-entries-list" role="region" aria-label={subtab === 'files' ? 'Tệp lớn nhất' : 'Thư mục Build và Dev'}>
      {entries.map(item => (
        <div className="disk-entry-row" key={item.path}>
          <div className="disk-entry-meta">
            <b title={item.name}>{item.name}</b>
            <small>{item.path}</small>
          </div>
          <div className="disk-entry-actions">
            <span className="status-badge muted">{formatBytes(item.bytes)}</span>
            <button type="button" className="inline-action-btn" disabled={busy} onClick={() => onReveal(item.path)}>
              <FolderOpen size={12} aria-hidden="true" /> Mở
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
