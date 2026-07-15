import { describe, expect, it } from 'vitest';
import { taskbarLayout } from './taskbar-layout';

describe('taskbarLayout', () => {
  it('uses at most two rows while adding columns for more metrics', () => {
    expect(taskbarLayout(5)).toEqual({ columns: 3, rows: 2, width: 300, height: 42 });
  });
});
