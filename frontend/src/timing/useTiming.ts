/**
 * React hooks for TimingManager
 */

import { useState, useEffect, useCallback } from 'react';
import { getTimingManager, type TimingManager } from './TimingManager';
import type { TransportState } from '../types';

/**
 * Hook to get the current interpolated beat
 * Updates on every animation frame when playing
 */
export function useBeat(): number {
  const [beat, setBeat] = useState(() => getTimingManager().getBeat());

  useEffect(() => {
    const manager = getTimingManager();
    return manager.subscribe(setBeat);
  }, []);

  return beat;
}

/**
 * Hook to get full interpolated transport state
 * Updates on every animation frame when playing
 */
export function useTransport(): TransportState {
  const [transport, setTransport] = useState(() => getTimingManager().getTransport());

  useEffect(() => {
    const manager = getTimingManager();
    return manager.subscribe(() => {
      setTransport(manager.getTransport());
    });
  }, []);

  return transport;
}

/**
 * Hook to sync server transport updates to TimingManager
 * Use this in your top-level component
 */
export function useTimingSync(): (transport: TransportState) => void {
  return useCallback((transport: TransportState) => {
    getTimingManager().sync(transport);
  }, []);
}

/**
 * Hook to get TimingManager instance directly
 */
export function useTimingManager(): TimingManager {
  return getTimingManager();
}
