/**
 * FugueList - Shows all active fugues with selection and controls
 *
 * Uses direct DOM manipulation for smooth progress bars without React re-renders.
 */

import React, { useRef, useEffect } from 'react';
import type { FugueInfo } from '../types';
import { getTimingManager } from '../timing';
import { startDrag } from '../api';
import './FugueList.css';

export interface FugueListProps {
  fugues: FugueInfo[];
  selectedId?: string;
  /** Instance id the fugues belong to — passed through to the drag IPC
   *  so Rust targets the right instance when looking up definitions. */
  instance: string;
  /** Session tempo, forwarded into the exported MIDI tempo meta event. */
  tempoBpm: number;
  onSelect: (id: string) => void;
  onCancel: (id: string) => void;
  onExport: (id: string) => void;
}

export const FugueList: React.FC<FugueListProps> = ({
  fugues,
  selectedId,
  instance,
  tempoBpm,
  onSelect,
  onCancel,
  onExport,
}) => {
  // Refs to directly update progress bar widths without React re-renders
  const progressRefs = useRef<Map<string, HTMLDivElement>>(new Map());

  // Subscribe to timing updates and directly manipulate DOM
  useEffect(() => {
    const timing = getTimingManager();

    const updateProgressBars = (currentBeat: number) => {
      const transport = timing.getTransport();

      for (const info of fugues) {
        const progressBar = progressRefs.current.get(info.id);
        if (!progressBar) continue;

        if (info.duration_beats === 0 || info.is_waiting) {
          progressBar.style.width = '0%';
          continue;
        }

        let localBeat = currentBeat - info.start_beat;

        // If DAW is looping and we appear to be before start, we're actually
        // in a later iteration. Calculate how many loop cycles have passed.
        if (localBeat < 0 && transport.is_looping) {
          const loopLength = transport.loop_end_beat - transport.loop_start_beat;
          if (loopLength > 0) {
            const loopsPassed = Math.ceil((info.start_beat - currentBeat) / loopLength);
            localBeat += loopsPassed * loopLength;
          }
        }

        if (localBeat < 0) {
          progressBar.style.width = '0%';
          continue;
        }

        // Wrap within duration for smooth looping
        const wrappedBeat = ((localBeat % info.duration_beats) + info.duration_beats) % info.duration_beats;
        const percent = (wrappedBeat / info.duration_beats) * 100;
        progressBar.style.width = `${percent}%`;
      }
    };

    return timing.subscribe(updateProgressBars);
  }, [fugues]);

  const getLoopDisplay = (info: FugueInfo): string => {
    if (info.total_loops === null) return '∞';
    return `${info.current_loop + 1}/${info.total_loops}`;
  };

  // Callback ref to store progress bar refs
  const setProgressRef = (id: string) => (el: HTMLDivElement | null) => {
    if (el) {
      progressRefs.current.set(id, el);
    } else {
      progressRefs.current.delete(id);
    }
  };

  // Distinguish click-to-select from press-and-drag: on mousedown, attach
  // document-level listeners that watch for movement past a small
  // threshold. If the cursor moves, we kick off the native OS drag (via
  // the wry IPC bridge) and remember that a drag happened; if it doesn't,
  // the usual onClick → onSelect fires. This matches the HTML5 drag UX
  // users already expect, without using HTML5 drag (which can't hand a
  // file off to Bitwig/Ableton from inside a webview).
  const DRAG_THRESHOLD_PX = 4;
  const pendingDragRef = useRef<{ x: number; y: number } | null>(null);
  const didDragRef = useRef(false);

  const beginPressDrag = (e: React.MouseEvent, onThreshold: () => void) => {
    if (e.button !== 0) return; // left click only
    pendingDragRef.current = { x: e.clientX, y: e.clientY };
    didDragRef.current = false;

    const moveHandler = (ev: MouseEvent) => {
      const start = pendingDragRef.current;
      if (!start) return;
      const dx = ev.clientX - start.x;
      const dy = ev.clientY - start.y;
      if (dx * dx + dy * dy > DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
        didDragRef.current = true;
        cleanup();
        onThreshold();
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

  const handleRowMouseDown = (e: React.MouseEvent, id: string) => {
    beginPressDrag(e, () => {
      startDrag({ instance, fugue_ids: [id], tempo_bpm: tempoBpm });
    });
  };

  const handleRowClick = (id: string) => {
    // If we just crossed the drag threshold on this gesture, the click
    // is really the tail end of a drag — swallow it so the row doesn't
    // also get selected.
    if (didDragRef.current) {
      didDragRef.current = false;
      return;
    }
    onSelect(id);
  };

  const handleDragAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    startDrag({ instance, active: true, tempo_bpm: tempoBpm });
  };

  if (fugues.length === 0) {
    return (
      <div className="fugue-list empty">
        <p className="empty-message">No active fugues</p>
        <p className="empty-hint">Queue a fugue to see it here</p>
      </div>
    );
  }

  return (
    <div className="fugue-list">
      {fugues.length > 1 && (
        <div className="fugue-list-header">
          <button
            className="drag-all-btn"
            onMouseDown={handleDragAll}
            title={`Drag all ${fugues.length} fugues as one .mid into your DAW`}
          >
            ⇣ Drag all <span className="drag-all-count">{fugues.length}</span>
          </button>
        </div>
      )}
      {fugues.map((info) => (
        <div
          key={info.id}
          className={`fugue-list-item ${selectedId === info.id ? 'selected' : ''} ${info.is_waiting ? 'waiting' : ''}`}
          onMouseDown={(e) => handleRowMouseDown(e, info.id)}
          onClick={() => handleRowClick(info.id)}
          title="Click to select · Press and drag to drop as .mid into your DAW"
        >
          <div className="item-main">
            <span className="item-tag">{info.tag || 'Untitled'}</span>
            <span className="item-loop">{getLoopDisplay(info)}</span>
          </div>

          <div className="item-progress">
            <div
              ref={setProgressRef(info.id)}
              className="progress-bar"
            />
          </div>

          <div className="item-controls">
            {info.is_waiting && (
              <span className="waiting-badge">Waiting</span>
            )}
            <button
              className="export-btn"
              onClick={(e) => {
                e.stopPropagation();
                onExport(info.id);
              }}
              title="Export as MIDI file to the exports folder"
            >
              ↓
            </button>
            <button
              className="cancel-btn"
              onClick={(e) => {
                e.stopPropagation();
                onCancel(info.id);
              }}
              title="Cancel this fugue"
            >
              ×
            </button>
          </div>
        </div>
      ))}
    </div>
  );
};

export default FugueList;
