import { describe, expect, it } from 'vitest';
import { sparklinePoints } from './sparkline';

describe('sparklinePoints', () => {
  it('normalizes a history into bounded SVG coordinates', () => {
    expect(sparklinePoints([0, 50, 100], 100, 40)).toBe('0,40 50,20 100,0');
  });
});
