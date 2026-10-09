import { afterEach, describe, expect, it, vi } from 'vitest';
import { subscribeToStream } from './tauri-stream';

const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });
describe('stream lifecycle', () => {
  it('keeps stale polling working when the wall clock moves backwards', async () => {
    vi.useFakeTimers();
    let monotonic = 0;
    vi.spyOn(performance, 'now').mockImplementation(() => monotonic);
    const fetch = vi.fn(async () => 1);
    const stop = subscribeToStream({ fetch, listen: async () => () => {}, onValue: vi.fn(), onError: vi.fn(), pollInterval: 1000, staleAfter: 2500 });
    await flush();
    vi.setSystemTime(Date.now() - 3600000);
    monotonic = 3000;
    await vi.advanceTimersByTimeAsync(3000);
    stop();
    expect(fetch.mock.calls.length).toBeGreaterThan(1);
  });
  it('does not refresh freshness from a cached process sequence after equal data', async () => {
    vi.useFakeTimers();
    let emit!: (value: { processSequence: number; rate: number }) => void;
    let cached = { processSequence: 1, rate: 0 };
    const activity = vi.fn();
    const values: number[] = [];
    const stop = subscribeToStream({ fetch: async () => cached, listen: async handler => { emit = handler; return () => {}; },
      sequence: value => value.processSequence, equal: (left, right) => left.rate === right.rate,
      onActivity: activity, onValue: value => values.push(value.rate), onError: vi.fn(), pollInterval: 1000, staleAfter: 2500 });
    await flush();
    cached = { processSequence: 2, rate: 0 };
    emit(cached);
    await vi.advanceTimersByTimeAsync(8000);
    stop();
    expect(values).toEqual([0]);
    expect(activity).toHaveBeenCalledTimes(2);
  });
  it('rejects stale or duplicate system sequences', async () => {
    let emit!: (value: { sequence: number }) => void;
    const values: number[] = [];
    const stop = subscribeToStream({ fetch: async () => ({ sequence: 1 }), listen: async handler => { emit = handler; return () => {}; }, sequence: value => value.sequence, onValue: value => values.push(value.sequence), onError: vi.fn() });
    await flush();
    emit({ sequence: 3 });
    emit({ sequence: 2 });
    emit({ sequence: 3 });
    stop();
    expect(values).toEqual([1, 3]);
  });
  it('keeps identical process heartbeats fresh without rendering duplicates', async () => {
    let emit!: (value: number) => void;
    const values: number[] = [];
    const activity = vi.fn();
    const stop = subscribeToStream({ fetch: async () => 1, listen: async handler => { emit = handler; return () => {}; }, onValue: value => values.push(value), onActivity: activity, onError: vi.fn() });
    await flush();
    emit(1);
    stop();
    expect(values).toEqual([1]);
    expect(activity).toHaveBeenCalledTimes(2);
  });
  it('unsubscribes a listener that resolves after disposal', async () => {
    const listener = deferred<() => void>();
    const unlisten = vi.fn();
    const stop = subscribeToStream({ fetch: async () => 0, listen: () => listener.promise, onValue: vi.fn(), onError: vi.fn() });
    stop();
    listener.resolve(unlisten);
    await flush();
    expect(unlisten).toHaveBeenCalledOnce();
  });
  it('never lets an older fetch overwrite a newer event', async () => {
    const request = deferred<number>();
    const values: number[] = [];
    let emit!: (value: number) => void;
    const stop = subscribeToStream({ fetch: () => request.promise, listen: async handler => { emit = handler; return () => {}; }, onValue: value => values.push(value), onError: vi.fn() });
    await flush();
    emit(2);
    request.resolve(1);
    await flush();
    stop();
    expect(values).toEqual([2]);
  });
  it('ignores in-flight values after disposal', async () => {
    const request = deferred<number>();
    const values: number[] = [];
    const stop = subscribeToStream({ fetch: () => request.promise, listen: async () => () => {}, onValue: value => values.push(value), onError: vi.fn() });
    await flush();
    stop();
    request.resolve(1);
    await flush();
    expect(values).toEqual([]);
  });
  it('allows only one stale polling request in flight', async () => {
    vi.useFakeTimers();
    const request = deferred<number>();
    const fetch = vi.fn(() => request.promise);
    const stop = subscribeToStream({ fetch, listen: async () => () => {}, onValue: vi.fn(), onError: vi.fn(), pollInterval: 1000, staleAfter: 2500 });
    await flush();
    await vi.advanceTimersByTimeAsync(10000);
    expect(fetch).toHaveBeenCalledOnce();
    stop();
  });
  it('suppresses duplicate poll values and surfaces listener failures', async () => {
    vi.useFakeTimers();
    const values: number[] = [];
    const errors: string[] = [];
    const stop = subscribeToStream({ fetch: async () => 1, listen: async () => { throw new Error('offline'); }, onValue: value => values.push(value), onError: error => errors.push(error), pollInterval: 1000, staleAfter: 2500 });
    await flush();
    await vi.advanceTimersByTimeAsync(7000);
    stop();
    expect(values).toEqual([1]);
    expect(errors).toContain('offline');
  });
});
