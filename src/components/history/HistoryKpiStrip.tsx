import { formatBytes } from '../../lib/format';
import type { HistoryRecord } from '../../lib/history';

export function HistoryKpiStrip({ records, diskAvailable }: { records: HistoryRecord[]; diskAvailable: boolean }) {
  if (!records.length) return null;
  const totals = records.reduce(
    (sum, item) => [sum[0] + item.bytes[0], sum[1] + item.bytes[1], sum[2] + item.bytes[2], sum[3] + item.bytes[3]],
    [0, 0, 0, 0],
  );
  const cards = [
    { label: 'Tổng tải xuống', value: formatBytes(totals[0]), tone: 'down' },
    { label: 'Tổng tải lên', value: formatBytes(totals[1]), tone: 'up' },
    { label: 'Tổng I/O đọc', value: diskAvailable ? formatBytes(totals[2]) : 'N/A', tone: 'read' },
    { label: 'Tổng I/O ghi', value: diskAvailable ? formatBytes(totals[3]) : 'N/A', tone: 'write' },
  ];
  return (
    <div className="history-kpi-strip" aria-label="Tổng hợp theo dữ liệu đang hiển thị">
      {cards.map(card => (
        <div className="history-kpi-card" key={card.label}>
          <span>{card.label}</span>
          <b className={card.tone}>{card.value}</b>
        </div>
      ))}
    </div>
  );
}
