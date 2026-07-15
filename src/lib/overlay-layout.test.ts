import { describe, expect, it } from 'vitest';
import { overlayLayout } from './overlay-layout';

describe('overlayLayout', () => {
  it('uses at most two columns and only enough rows for visible metrics', () => {
    expect(overlayLayout(1)).toEqual({ columns: 1, rows: 1, width: 146, height: 44 });
    expect(overlayLayout(3)).toEqual({ columns: 2, rows: 2, width: 296, height: 84 });
  });
});
