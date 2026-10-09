export type HistoryRecord = {
  timestampSecs: number;
  durationMillis: number;
  bytes: [number, number, number, number];
};

export function historyRate(bytes: number, durationMillis: number): number | null {
  return durationMillis > 0 ? bytes * 1000 / durationMillis : null;
}

export function buildHistoryCsv(records: HistoryRecord[], diskAvailable: boolean): string {
  const rows = records.map(record => [
    new Date(record.timestampSecs * 1000).toISOString(),
    record.durationMillis,
    record.bytes[0], record.bytes[1],
    diskAvailable ? record.bytes[2] : 'N/A',
    diskAvailable ? record.bytes[3] : 'N/A',
  ].join(','));
  return '\uFEFF' + ['UTC,DurationMillis,NetworkDownBytes,NetworkUpBytes,DiskReadBytes,DiskWriteBytes', ...rows].join('\r\n');
}
