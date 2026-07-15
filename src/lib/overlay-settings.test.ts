import { describe, expect, it } from 'vitest';
import { normalizeOverlaySettings } from './overlay-settings';

describe('normalizeOverlaySettings', () => {
  it('keeps the overlay readable without allowing an opaque panel', () => {
    expect(normalizeOverlaySettings({ opacity: 12 }).opacity).toBe(35);
    expect(normalizeOverlaySettings({ opacity: 140 }).opacity).toBe(92);
  });
});
