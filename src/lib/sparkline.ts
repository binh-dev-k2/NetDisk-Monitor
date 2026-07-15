export function sparklinePoints(values: number[], width: number, height: number): string {
  if (values.length === 0) return '';
  const max = Math.max(...values, 1);
  const denominator = Math.max(values.length - 1, 1);
  return values
    .map((value, index) => `${Math.round((index / denominator) * width)},${Math.round(height - (Math.max(value, 0) / max) * height)}`)
    .join(' ');
}
