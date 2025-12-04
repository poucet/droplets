/**
 * FugueList - Shows all active fugues with selection and controls
 */

import React from 'react';
import type { FugueInfo, TransportState } from '../types';
import './FugueList.css';

export interface FugueListProps {
  fugues: FugueInfo[];
  selectedId?: bigint;
  transport: TransportState;
  onSelect: (id: bigint) => void;
  onCancel: (id: bigint) => void;
}

export const FugueList: React.FC<FugueListProps> = ({
  fugues,
  selectedId,
  transport,
  onSelect,
  onCancel,
}) => {
  const getLoopDisplay = (info: FugueInfo): string => {
    if (info.total_loops === null) return '∞';
    return `${info.current_loop + 1}/${info.total_loops}`;
  };

  const getProgressPercent = (info: FugueInfo): number => {
    if (info.duration_beats === 0 || info.is_waiting) return 0;

    // For interval-based quantization, use transport % interval
    if (info.quantize_interval_beats !== null) {
      const effectiveInterval = Math.max(info.quantize_interval_beats, info.duration_beats);
      const localBeat = transport.beat % effectiveInterval;
      const progress = Math.max(0, Math.min(localBeat, info.duration_beats));
      return (progress / info.duration_beats) * 100;
    }

    // For Immediate mode, use traditional calculation
    const localBeat = transport.beat - info.start_beat;
    const progress = Math.max(0, localBeat % info.duration_beats);
    return (progress / info.duration_beats) * 100;
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
          key={String(info.id)}
          className={`fugue-list-item ${selectedId === info.id ? 'selected' : ''} ${info.is_waiting ? 'waiting' : ''}`}
          onClick={() => onSelect(info.id)}
        >
          <div className="item-main">
            <span className="item-tag">{info.tag || 'Untitled'}</span>
            <span className="item-loop">{getLoopDisplay(info)}</span>
          </div>

          <div className="item-progress">
            <div
              className="progress-bar"
              style={{ width: `${getProgressPercent(info)}%` }}
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
