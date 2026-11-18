import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ProcessGroup } from '../types';

const MAX_CACHE = 512;

export function useProcessIcons(groups: ProcessGroup[], active: boolean): Record<string, string> {
  const mountedRef = useRef(true);
  const cacheRef = useRef<Map<string, string | null>>(new Map());
  const inFlightRef = useRef<Set<string>>(new Set());
  const [icons, setIcons] = useState<Record<string, string>>({});

  useEffect(() => {
    mountedRef.current = true;
    const clearCache = () => {
      cacheRef.current.clear();
      inFlightRef.current.clear();
      setIcons({});
    };
    window.addEventListener('netdisk:caches-cleared', clearCache);
    return () => {
      mountedRef.current = false;
      window.removeEventListener('netdisk:caches-cleared', clearCache);
    };
  }, []);

  useEffect(() => {
    if (!active || groups.length === 0) return;

    const missing = groups
      .filter(group => !inFlightRef.current.has(group.name) && !cacheRef.current.has(group.name))
      .slice(0, 64)
      .map(group => ({
        name: group.name,
        pid: group.children.find(child => child.active)?.pid ?? group.pid,
      }));

    if (missing.length === 0) return;

    for (const item of missing) inFlightRef.current.add(item.name);

    void invoke<Record<string, string | null> | null>('get_process_icons', { items: missing })
      .then(result => {
        if (!mountedRef.current) return;
        const updates: Record<string, string> = {};
        if (result && typeof result === 'object') {
          for (const [name, uri] of Object.entries(result)) {
            if (!cacheRef.current.has(name) && cacheRef.current.size >= MAX_CACHE) {
              const oldest = cacheRef.current.keys().next().value;
              if (oldest !== undefined) cacheRef.current.delete(oldest);
            }
            cacheRef.current.set(name, uri ?? null);
            if (uri) updates[name] = uri;
          }
          for (const item of missing) {
            if (!Object.prototype.hasOwnProperty.call(result, item.name)) {
              cacheRef.current.set(item.name, null);
            }
          }
        } else {
          for (const item of missing) cacheRef.current.set(item.name, null);
        }
        if (Object.keys(updates).length > 0) {
          setIcons(prev => {
            const next = { ...prev, ...updates };
            const keys = Object.keys(next);
            if (keys.length > MAX_CACHE) {
              for (const key of keys.slice(0, keys.length - MAX_CACHE)) delete next[key];
            }
            return next;
          });
        }
      })
      .catch(() => {
        if (!mountedRef.current) return;
        for (const item of missing) cacheRef.current.set(item.name, null);
      })
      .finally(() => {
        for (const item of missing) inFlightRef.current.delete(item.name);
      });
  }, [active, groups]);

  return icons;
}
