import type { SpaceNode } from '../types';

export type TreemapTile = {
  node: SpaceNode;
  x: number;
  y: number;
  w: number;
  h: number;
  sharePercent: number;
};

type Rect = { x: number; y: number; w: number; h: number };

function splitBinary(
  items: Array<{ node: SpaceNode; totalShare: number }>,
  rect: Rect,
  out: TreemapTile[],
): void {
  if (items.length === 0 || rect.w <= 0 || rect.h <= 0) return;
  if (items.length === 1) {
    out.push({ node: items[0].node, x: rect.x, y: rect.y, w: rect.w, h: rect.h, sharePercent: items[0].totalShare });
    return;
  }
  const sum = items.reduce((acc, item) => acc + item.node.bytes, 0);
  if (sum <= 0) return;
  let running = 0;
  let splitIdx = 1;
  const half = sum / 2;
  for (let i = 0; i < items.length - 1; i++) {
    running += items[i].node.bytes;
    splitIdx = i + 1;
    if (running >= half) break;
  }
  const left = items.slice(0, splitIdx);
  const right = items.slice(splitIdx);
  const ratio = left.reduce((acc, item) => acc + item.node.bytes, 0) / sum;
  if (rect.w >= rect.h) {
    const leftW = rect.w * ratio;
    splitBinary(left, { x: rect.x, y: rect.y, w: leftW, h: rect.h }, out);
    splitBinary(right, { x: rect.x + leftW, y: rect.y, w: rect.w - leftW, h: rect.h }, out);
  } else {
    const topH = rect.h * ratio;
    splitBinary(left, { x: rect.x, y: rect.y, w: rect.w, h: topH }, out);
    splitBinary(right, { x: rect.x, y: rect.y + topH, w: rect.w, h: rect.h - topH }, out);
  }
}

export function compactTreemapNodes(
  nodes: ReadonlyArray<SpaceNode>,
  minShare = 0.015,
): SpaceNode[] {
  const sorted = nodes
    .filter(node => Number.isFinite(node.bytes) && node.bytes > 0)
    .sort((a, b) => b.bytes - a.bytes);
  if (!sorted.length) return [];
  const total = sorted.reduce((sum, node) => sum + node.bytes, 0);
  if (total <= 0) return [];
  const kept: SpaceNode[] = [sorted[0]];
  let tailBytes = 0;
  for (let i = 1; i < sorted.length; i++) {
    const item = sorted[i];
    if (item.path !== '' && item.bytes / total >= minShare) kept.push(item);
    else tailBytes += item.bytes;
  }
  if (tailBytes > 0) {
    if (kept[0].path === '') kept[0] = { ...kept[0], bytes: kept[0].bytes + tailBytes };
    else kept.push({ name: 'Các mục khác', path: '', bytes: tailBytes, isDir: false });
  }
  return kept;
}

export function layoutTreemap(
  nodes: ReadonlyArray<SpaceNode>,
  width = 100,
  height = 100,
): TreemapTile[] {
  const positive = nodes.filter(node => Number.isFinite(node.bytes) && node.bytes > 0);
  if (!positive.length || width <= 0 || height <= 0) return [];
  const total = positive.reduce((sum, node) => sum + node.bytes, 0);
  if (total <= 0) return [];
  const sorted = [...positive]
    .sort((a, b) => b.bytes - a.bytes)
    .map(node => ({ node, totalShare: (node.bytes / total) * 100 }));
  const tiles: TreemapTile[] = [];
  splitBinary(sorted, { x: 0, y: 0, w: width, h: height }, tiles);
  return tiles;
}
