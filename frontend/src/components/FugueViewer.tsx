/**
 * FugueViewer - Wrapper for viewing active fugues with transport sync
 */

import React, { useMemo } from 'react';
import { FugueGrid } from './FugueGrid';
import type { FugueDefinition, FugueInfo } from '../types';
import { useBeat, useTimingManager } from '../timing';
import './FugueViewer.css';

export interface FugueViewerProps {
  fugue: FugueDefinition;
  info?: FugueInfo;
  onEdit?: (fugue: FugueDefinition) => void;
}

export const FugueViewer: React.FC<FugueViewerProps> = ({
  fugue,
  info,
  onEdit,
}) => {
  // Client-side interpolated beat for smooth playhead animation
  const currentBeat = useBeat();
  const timing = useTimingManager();

  // Calculate playhead position using client-side interpolated beat
  const playheadBeat = useMemo(() => {
    if (!info || info.is_waiting) return undefined;
    const localBeat = currentBeat - info.start_beat;
    // Wrap within duration for looping
    const wrappedBeat = localBeat % fugue.duration_beats;
    return Math.max(0, Math.min(wrappedBeat, fugue.duration_beats));
  }, [info, currentBeat, fugue.duration_beats]);

  const loopDisplay = useMemo(() => {
    if (!info) return null;
    if (info.total_loops === null) return '∞';
    return `${info.current_loop + 1}/${info.total_loops}`;
  }, [info]);

  return (
    <div className="fugue-viewer">
      <div className="viewer-header">
        <span className="viewer-tag">{fugue.tag || 'Untitled'}</span>
        {loopDisplay && (
          <span className="viewer-loop">Loop: {loopDisplay}</span>
        )}
        <span className="viewer-duration">{fugue.duration_beats} beats</span>
        {info?.is_waiting && (
          <span className="viewer-waiting">Waiting for quantize...</span>
        )}
        {onEdit && (
          <button className="edit-btn" onClick={() => onEdit(fugue)}>
            Edit
          </button>
        )}
      </div>

      <FugueGrid
        events={fugue.events}
        durationBeats={fugue.duration_beats}
        playheadBeat={playheadBeat}
        isPlaying={timing.isPlaying() && !info?.is_waiting}
        mode="view"
        beatsPerBar={timing.getTimeSigNumerator()}
        pixelsPerBeat={50}
        noteHeight={14}
      />
    </div>
  );
};

export default FugueViewer;
