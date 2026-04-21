/**
 * FugueViewer - Wrapper for viewing active fugues with transport sync
 *
 * Uses client-side timing interpolation for smooth playhead animation.
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { FugueGrid } from './FugueGrid';
import type { FugueDefinition, FugueInfo } from '../types';
import { getTimingManager, useTimingManager } from '../timing';
import { startDrag } from '../api';
import './FugueViewer.css';

// Persist the debug-overlay toggle across reloads without touching server
// settings. localStorage is a pragmatic choice for a dev-only affordance.
const DEBUG_OVERLAY_KEY = 'droplets.debug.playhead-overlay';

function readDebugOverlayPref(): boolean {
  try {
    return localStorage.getItem(DEBUG_OVERLAY_KEY) === '1';
  } catch {
    return false;
  }
}

function writeDebugOverlayPref(enabled: boolean) {
  try {
    localStorage.setItem(DEBUG_OVERLAY_KEY, enabled ? '1' : '0');
  } catch {
    // Storage disabled (private mode, etc.) — fall back to session-only.
  }
}

export interface FugueViewerProps {
  fugue: FugueDefinition;
  info?: FugueInfo;
  onEdit?: (fugue: FugueDefinition) => void;
  onCancel?: (id: string) => void;
  onExport?: (id: string) => void;
  /** Instance id for the drag IPC so Rust finds the right definitions. */
  instance?: string;
  /** Session tempo forwarded into the exported MIDI tempo meta. */
  tempoBpm?: number;
}

export const FugueViewer: React.FC<FugueViewerProps> = ({
  fugue,
  info,
  onEdit,
  onCancel,
  onExport,
  instance,
  tempoBpm,
}) => {
  const timing = useTimingManager();

  // Show playhead only when fugue is actively playing (not waiting)
  const showPlayhead = info !== undefined && !info.is_waiting;

  // Dev-only overlay: live-display the numbers feeding FugueGrid's
  // playhead math so drift/misalignment bugs are debuggable without a
  // REPL. Persisted per-browser via localStorage; ignore in production
  // if you want by never clicking the toggle.
  const [debugOverlay, setDebugOverlay] = useState<boolean>(() =>
    readDebugOverlayPref()
  );
  const toggleDebugOverlay = () => {
    setDebugOverlay(v => {
      writeDebugOverlayPref(!v);
      return !v;
    });
  };
  const debugRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!debugOverlay) return;
    const mgr = getTimingManager();
    const startBeat = info?.start_beat;
    const dur = fugue.duration_beats;
    const update = (currentBeat: number) => {
      const el = debugRef.current;
      if (!el) return;
      const parts: string[] = [];
      parts.push(`now ${currentBeat.toFixed(3)}`);
      if (typeof startBeat === 'number') {
        const diff = currentBeat - startBeat;
        const phase =
          dur > 0 ? ((diff % dur) + dur) % dur : diff;
        parts.push(`start ${startBeat.toFixed(3)}`);
        parts.push(`dur ${dur.toFixed(3)}`);
        parts.push(`phase ${phase.toFixed(3)}`);
      } else {
        parts.push('start —');
      }
      el.textContent = parts.join(' · ');
    };
    // Immediately paint current values so the overlay isn't blank while
    // the transport is stopped (subscribe only pings while playing).
    update(mgr.getBeat());
    return mgr.subscribe(update);
  }, [debugOverlay, info?.start_beat, fugue.duration_beats]);

  const loopDisplay = useMemo(() => {
    if (!info) return null;
    if (info.total_loops === null) return '∞';
    return `${info.current_loop + 1}/${info.total_loops}`;
  }, [info]);

  // Header is a drag source for the currently-selected fugue. Press-and-
  // drag with a small movement threshold so plain header clicks don't
  // accidentally initiate a drag (same pattern FugueList uses per-row).
  const DRAG_THRESHOLD_PX = 4;
  const pendingDragRef = useRef<{ x: number; y: number } | null>(null);
  const handleHeaderMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0 || !instance) return;
    pendingDragRef.current = { x: e.clientX, y: e.clientY };
    const moveHandler = (ev: MouseEvent) => {
      const start = pendingDragRef.current;
      if (!start) return;
      const dx = ev.clientX - start.x;
      const dy = ev.clientY - start.y;
      if (dx * dx + dy * dy > DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
        cleanup();
        const idStr = String(fugue.id);
        startDrag({ instance, fugue_ids: [idStr], tempo_bpm: tempoBpm ?? 120 });
      }
    };
    const upHandler = () => cleanup();
    const cleanup = () => {
      pendingDragRef.current = null;
      document.removeEventListener('mousemove', moveHandler);
      document.removeEventListener('mouseup', upHandler);
    };
    document.addEventListener('mousemove', moveHandler);
    document.addEventListener('mouseup', upHandler);
  };

  return (
    <div className="fugue-viewer">
      <div
        className="viewer-header"
        onMouseDown={handleHeaderMouseDown}
        title={instance ? 'Press and drag to drop as .mid into your DAW' : undefined}
      >
        <span className="viewer-tag">{fugue.tag || 'Untitled'}</span>
        {loopDisplay && (
          <span className="viewer-loop">Loop: {loopDisplay}</span>
        )}
        <span className="viewer-duration">{fugue.duration_beats} beats</span>
        {info?.is_waiting && (
          <span className="viewer-waiting">Waiting for quantize...</span>
        )}
        {debugOverlay && (
          <span className="viewer-debug" ref={debugRef} onMouseDown={(e) => e.stopPropagation()}>
            …
          </span>
        )}
        <span className="viewer-spacer" />
        <button
          className="debug-btn"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            toggleDebugOverlay();
          }}
          title={debugOverlay ? 'Hide playhead debug overlay' : 'Show playhead debug overlay'}
          aria-pressed={debugOverlay}
        >
          🐞
        </button>
        {onEdit && (
          <button className="edit-btn" onClick={() => onEdit(fugue)}>
            Edit
          </button>
        )}
        {onExport && info && (
          <button
            className="export-btn"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onExport(info.id);
            }}
            title="Export as MIDI file to the exports folder"
          >
            ↓
          </button>
        )}
        {onCancel && info && (
          <button
            className="cancel-btn"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onCancel(info.id);
            }}
            title="Cancel this fugue"
          >
            ×
          </button>
        )}
      </div>

      <FugueGrid
        events={fugue.events}
        durationBeats={fugue.duration_beats}
        startBeat={info?.start_beat}
        showPlayhead={showPlayhead}
        mode="view"
        beatsPerBar={timing.getTimeSigNumerator()}
        pixelsPerBeat={50}
        noteHeight={14}
      />
    </div>
  );
};

export default FugueViewer;
