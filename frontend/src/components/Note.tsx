/**
 * Note - Presentational component for a single note in the piano roll
 *
 * Renders either:
 * - View mode: Simple rectangle
 * - Edit mode: Rectangle with drag handles for move/resize
 */

import React from 'react';

export type DragType = 'move' | 'resize-start' | 'resize-end';

export interface NoteProps {
  x: number;
  y: number;
  width: number;
  opacity: number;
  noteHeight: number;
  mode: 'view' | 'edit';
  isDragging: boolean;
  onDragStart?: (type: DragType, e: React.MouseEvent) => void;
}

const HANDLE_WIDTH = 6;

export const Note: React.FC<NoteProps> = ({
  x,
  y,
  width,
  opacity,
  noteHeight,
  mode,
  isDragging,
  onDragStart,
}) => {
  if (mode === 'edit' && onDragStart) {
    return (
      <g className={`note-group ${isDragging ? 'dragging' : ''}`}>
        {/* Main note body - drag to move */}
        <rect
          x={x + HANDLE_WIDTH}
          y={y + 1}
          width={Math.max(width - HANDLE_WIDTH * 2, 2)}
          height={noteHeight - 2}
          className="note-cell note-body"
          style={{ opacity }}
          onMouseDown={(e) => onDragStart('move', e)}
        />
        {/* Left resize handle */}
        <rect
          x={x}
          y={y + 1}
          width={HANDLE_WIDTH}
          height={noteHeight - 2}
          rx={2}
          className="note-cell note-handle note-handle-start"
          style={{ opacity }}
          onMouseDown={(e) => onDragStart('resize-start', e)}
        />
        {/* Right resize handle */}
        <rect
          x={x + width - HANDLE_WIDTH}
          y={y + 1}
          width={HANDLE_WIDTH}
          height={noteHeight - 2}
          rx={2}
          className="note-cell note-handle note-handle-end"
          style={{ opacity }}
          onMouseDown={(e) => onDragStart('resize-end', e)}
        />
      </g>
    );
  }

  // View mode - simple rectangle
  return (
    <rect
      x={x}
      y={y + 1}
      width={width}
      height={noteHeight - 2}
      rx={2}
      className="note-cell"
      style={{ opacity }}
    />
  );
};

export default Note;
