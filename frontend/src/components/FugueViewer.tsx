/**
 * FugueViewer - Wrapper for viewing active fugues with transport sync
 *
 * Uses client-side timing interpolation for smooth playhead animation.
 */

import React, { useMemo } from 'react';
import { FugueGrid } from './FugueGrid';
import type { FugueDefinition, FugueInfo } from '../types';
import { useTimingManager } from '../timing';
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
  const timing = useTimingManager();

  // Show playhead only when fugue is actively playing (not waiting)
  const showPlayhead = info !== undefined && !info.is_waiting;

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
