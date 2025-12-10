/**
 * FugueList - Shows all active fugues with selection and controls
 *
 * Uses direct DOM manipulation for smooth progress bars without React re-renders.
 */

import React, { useRef, useEffect } from 'react';
import type { FugueInfo } from '../types';
import { getTimingManager } from '../timing';
import './FugueList.css';

export interface FugueListProps {
  fugues: FugueInfo[];
  selectedId?: string;
  onSelect: (id: string) => void;
  onCancel: (id: string) => void;
}

export const FugueList: React.FC<FugueListProps> = ({
  fugues,
  selectedId,
  onSelect,
  onCancel,
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
      {fugues.map((info) => (
        <div
          key={info.id}
          className={`fugue-list-item ${selectedId === info.id ? 'selected' : ''} ${info.is_waiting ? 'waiting' : ''}`}
          onClick={() => onSelect(info.id)}
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
