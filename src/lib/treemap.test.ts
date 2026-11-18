import { describe, expect, it } from 'vitest';
import { compactTreemapNodes, layoutTreemap } from './treemap';
import type { SpaceNode } from '../types';

const node = (name: string, bytes: number, isDir = true): SpaceNode => ({
  name,
  path: `C:\\Test\\${name}`,
  bytes,
  isDir,
});

describe('layoutTreemap', () => {
  it('returns empty array for empty or zero-byte inputs', () => {
    expect(layoutTreemap([])).toEqual([]);
    expect(layoutTreemap([node('empty', 0)])).toEqual([]);
  });

  it('fills full rectangle cleanly across single and skewed nodes', () => {
    const single = layoutTreemap([node('only', 500)], 100, 100);
    expect(single).toHaveLength(1);
    expect(single[0]).toMatchObject({ x: 0, y: 0, w: 100, h: 100, sharePercent: 100 });

    const skewed = layoutTreemap([
      node('big', 700),
      node('mid', 200),
      node('small', 100),
      node('ignored', 0),
    ]);
    expect(skewed).toHaveLength(3);
    const totalArea = skewed.reduce((sum, tile) => sum + tile.w * tile.h, 0);
    expect(totalArea).toBeCloseTo(10_000, 3);
    expect(skewed[0].sharePercent).toBeCloseTo(70, 3);
    for (const tile of skewed) {
      expect(tile.w).toBeGreaterThan(0);
      expect(tile.h).toBeGreaterThan(0);
      expect(tile.x + tile.w).toBeLessThanOrEqual(100.001);
      expect(tile.y + tile.h).toBeLessThanOrEqual(100.001);
    }
  });

  it('folds items below minShare into a single non-interactive summary node', () => {
    const compacted = compactTreemapNodes([
      node('major', 980),
      node('tiny-a', 10),
      node('tiny-b', 5),
      { name: 'Các mục khác', path: '', bytes: 5, isDir: false },
    ]);
    expect(compacted).toEqual([
      node('major', 980),
      { name: 'Các mục khác', path: '', bytes: 20, isDir: false },
    ]);
  });
});
