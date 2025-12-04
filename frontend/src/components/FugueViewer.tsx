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
  // Calculate playhead position within the fugue
  // This requires knowing when the fugue started, which we approximate from progress
  const playheadBeat = useMemo(() => {
    if (!info || !transport.playing) return undefined;

    // Use progress_beats from info as the current position within the fugue
    return info.progress_beats;
  }, [info, transport.playing]);

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
