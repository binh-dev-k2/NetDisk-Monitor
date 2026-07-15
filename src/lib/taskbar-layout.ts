export function taskbarLayout(itemCount: number): { columns: number; rows: number; width: number; height: number } {
  const rows = Math.min(Math.max(itemCount, 1), 2);
  const columns = Math.ceil(Math.max(itemCount, 1) / rows);
  const height = rows === 1 ? 28 : 42;
  return { columns, rows, width: columns * 100, height };
}
