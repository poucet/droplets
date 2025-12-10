/**
 * TimingManager - Client-side beat interpolation for smooth UI animation
 *
 * The server sends transport updates at a relatively slow rate. This class
 * interpolates beats client-side based on tempo and wall-clock time to provide
 * smooth animation. Server updates sync the state periodically.
 */

import type { TransportState } from '../types';

export type TimingListener = (beat: number) => void;

export class TimingManager {
  private transport: TransportState = {
    beat: 0,
    tempo: 120,
    playing: false,
    time_sig_numerator: 4,
  };

  // Wall-clock time (ms) when we last synced with server
  private lastSyncTime: number = 0;
  // Beat value at last sync
  private lastSyncBeat: number = 0;

  // Animation frame handle
  private animationFrame: number | null = null;

  // Listeners for beat updates
  private listeners: Set<TimingListener> = new Set();

  // Current interpolated beat (cached for getters)
  private currentBeat: number = 0;

  /**
   * Update transport state from server
   * This syncs our interpolation baseline
   */
  sync(transport: TransportState): void {
    this.transport = transport;
    this.lastSyncTime = performance.now();
    this.lastSyncBeat = transport.beat;
    this.currentBeat = transport.beat;

    // Start or stop animation based on playing state
    if (transport.playing && this.animationFrame === null) {
      this.startAnimation();
    } else if (!transport.playing && this.animationFrame !== null) {
      this.stopAnimation();
      // Notify listeners of final position
      this.notifyListeners();
    }
  }

  /**
   * Get current interpolated beat
   */
  getBeat(): number {
    return this.currentBeat;
  }

  /**
   * Get current tempo
   */
  getTempo(): number {
    return this.transport.tempo;
  }

  /**
   * Get whether transport is playing
   */
  isPlaying(): boolean {
    return this.transport.playing;
  }

  /**
   * Get time signature numerator
   */
  getTimeSigNumerator(): number {
    return this.transport.time_sig_numerator;
  }

  /**
   * Get full transport state with interpolated beat
   */
  getTransport(): TransportState {
    return {
      ...this.transport,
      beat: this.currentBeat,
    };
  }

  /**
   * Subscribe to beat updates
   * Returns unsubscribe function
   */
  subscribe(listener: TimingListener): () => void {
    this.listeners.add(listener);
    // Immediately notify with current beat
    listener(this.currentBeat);
    return () => this.listeners.delete(listener);
  }

  /**
   * Clean up resources
   */
  destroy(): void {
    this.stopAnimation();
    this.listeners.clear();
  }

  private startAnimation(): void {
    const tick = () => {
      this.updateBeat();
      this.notifyListeners();
      this.animationFrame = requestAnimationFrame(tick);
    };
    this.animationFrame = requestAnimationFrame(tick);
  }

  private stopAnimation(): void {
    if (this.animationFrame !== null) {
      cancelAnimationFrame(this.animationFrame);
      this.animationFrame = null;
    }
  }

  private updateBeat(): void {
    if (!this.transport.playing) {
      this.currentBeat = this.transport.beat;
      return;
    }

    // Calculate elapsed time since last sync
    const now = performance.now();
    const elapsedMs = now - this.lastSyncTime;

    // Convert to beats: (ms / 1000) * (bpm / 60) = ms * bpm / 60000
    const elapsedBeats = (elapsedMs * this.transport.tempo) / 60000;

    this.currentBeat = this.lastSyncBeat + elapsedBeats;
  }

  private notifyListeners(): void {
    for (const listener of this.listeners) {
      listener(this.currentBeat);
    }
  }
}

// Singleton instance
let instance: TimingManager | null = null;

export function getTimingManager(): TimingManager {
  if (!instance) {
    instance = new TimingManager();
  }
  return instance;
}
