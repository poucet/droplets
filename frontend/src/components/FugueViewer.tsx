/**
 * FugueViewer - Wrapper for viewing active fugues with transport sync
 *
 * Uses client-side timing interpolation for smooth playhead animation.
 */

import React, { useMemo, useRef } from 'react';
import { FugueGrid } from './FugueGrid';
import type { FugueDefinition, FugueInfo } from '../types';
import { useTimingManager } from '../timing';
import { startDrag } from '../api';
import './FugueViewer.css';

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
        <span className="viewer-spacer" />
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
