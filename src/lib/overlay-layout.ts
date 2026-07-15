export function overlayLayout(itemCount: number): { columns: number; rows: number; width: number; height: number } {
  const columns = itemCount > 1 ? 2 : 1;
  const rows = Math.max(1, Math.ceil(itemCount / columns));
  return { columns, rows, width: columns === 1 ? 146 : 296, height: 4 + rows * 40 };
}
