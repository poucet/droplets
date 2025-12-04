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
}

export const FugueViewer: React.FC<FugueViewerProps> = ({
  fugue,
  info,
  transport,
}) => {
  // Calculate playhead position within the fugue from transport position
  // Uses the interval-based model: playhead = transport_beat % interval
  const playheadBeat = useMemo(() => {
    if (!info || info.is_waiting) return undefined;

    // For interval-based quantization, use transport % interval
    // This ensures the playhead is always synchronized to the grid
    if (info.quantize_interval_beats !== null) {
      // Use the larger of interval or duration for the modulo
      // This handles cases where the fugue is shorter than the interval
      const effectiveInterval = Math.max(info.quantize_interval_beats, fugue.duration_beats);
      const localBeat = transport.beat % effectiveInterval;
      return Math.max(0, Math.min(localBeat, fugue.duration_beats));
    }

    // For Immediate mode (no interval), use the traditional calculation
    const localBeat = transport.beat - info.start_beat;
    return Math.max(0, Math.min(localBeat % fugue.duration_beats, fugue.duration_beats));
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
