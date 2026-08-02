import { startTransition, useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type UseTauriStreamOptions<T> = {
  defaultValue: T;
  normalize?: (value: T) => T;
  transition?: boolean;
};

export function useTauriStream<T>(
  command: string,
  eventName: string,
  options: UseTauriStreamOptions<T>,
): [T, Dispatch<SetStateAction<T>>] {
  const { defaultValue, normalize, transition = false } = options;
  const [value, setValue] = useState<T>(defaultValue);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;

    const apply = (next: T) => {
      if (disposed) return;
      const normalized = normalize ? normalize(next) : next;
      if (transition) {
        startTransition(() => setValue(normalized));
      } else {
        setValue(normalized);
      }
    };

    void invoke<T>(command).then(apply).catch(console.error);
    void listen<T>(eventName, event => apply(event.payload)).then(listener => {
      unlisten = listener;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [command, eventName, normalize, transition]);

  return [value, setValue];
}
