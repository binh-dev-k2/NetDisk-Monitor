type StreamOptions<T> = {
  fetch: () => Promise<T>;
  listen: (handler: (value: T) => void) => Promise<() => void>;
  onValue: (value: T) => void;
  onError: (error: string) => void;
  onActivity?: () => void;
  pollInterval?: number;
  staleAfter?: number;
  sequence?: (value: T) => number;
  equal?: (left: T, right: T) => boolean;
};

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function subscribeToStream<T>(options: StreamOptions<T>): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  let inFlight = false;
  let eventRevision = 0;
  let lastEventAt = performance.now();
  let previous: T | undefined;
  let hasValue = false;
  let acceptedSequence: number | undefined;
  const report = (error: unknown) => { if (!disposed) options.onError(errorMessage(error)); };
  const apply = (next: T) => {
    if (disposed) return;
    if (options.sequence) {
      const sequence = options.sequence(next);
      if (!Number.isFinite(sequence) || (acceptedSequence !== undefined && sequence <= acceptedSequence)) return;
      acceptedSequence = sequence;
    }
    options.onActivity?.();
    if (hasValue && (options.equal ?? Object.is)(previous as T, next)) return;
    previous = next;
    hasValue = true;
    options.onValue(next);
  };
  const fetch = async () => {
    if (disposed || inFlight) return;
    inFlight = true;
    const revision = eventRevision;
    try {
      const next = await options.fetch();
      if (revision === eventRevision) apply(next);
    } catch (error) { report(error); }
    finally { inFlight = false; }
  };
  void (async () => {
    try {
      const listener = await options.listen(value => {
        if (disposed) return;
        eventRevision++;
        lastEventAt = performance.now();
        apply(value);
      });
      if (disposed) listener();
      else unlisten = listener;
    } catch (error) { report(error); }
    await fetch();
  })();
  const timer = options.pollInterval ? setInterval(() => {
    if (performance.now() - lastEventAt >= (options.staleAfter ?? options.pollInterval!)) void fetch();
  }, options.pollInterval) : undefined;
  return () => {
    disposed = true;
    if (timer) clearInterval(timer);
    unlisten?.();
  };
}
