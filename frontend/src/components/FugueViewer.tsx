/**
 * FugueViewer - Wrapper for viewing active fugues with transport sync
 */

import React, { useMemo } from 'react';
import { FugueGrid } from './FugueGrid';
import type { FugueDefinition, FugueInfo, TransportState } from '../types';
import './FugueViewer.css';

export interface FugueViewerProps {
  fugue: FugueDefinition;
  info?: FugueInfo;
  transport: TransportState;
  onEdit?: (fugue: FugueDefinition) => void;
}

export const FugueViewer: React.FC<FugueViewerProps> = ({
  fugue,
  info,
  transport,
  onEdit,
}) => {
  // Calculate playhead position within the fugue from transport position
  // Uses start_beat which is updated by the audio thread on each loop iteration
  const playheadBeat = useMemo(() => {
    if (!info || info.is_waiting) return undefined;

    // Calculate local position from transport beat and the fugue's current start_beat
    // start_beat advances by duration_beats on each loop, so this gives us the
    // correct position within the current loop iteration
    const localBeat = transport.beat - info.start_beat;

    // Clamp to valid range within the fugue duration
    return Math.max(0, Math.min(localBeat, fugue.duration_beats));
  }, [info, transport.beat, fugue.duration_beats]);

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
        isPlaying={transport.playing && !info?.is_waiting}
        mode="view"
        beatsPerBar={transport.time_sig_numerator}
        pixelsPerBeat={50}
        noteHeight={14}
      />
    </div>
  );
};

export default FugueViewer;
