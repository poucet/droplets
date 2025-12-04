/**
 * NoteLayer - Manages note interaction and coordinates Note components
 *
 * Responsible for:
 * - Drag state management (only one note can be dragged at a time)
 * - Click-to-add/remove notes
 * - Event updates when notes are modified
 */

import React, { useCallback, useState, useEffect } from 'react';
import type { TimedFugueEvent } from '../types';
import { Note, DragType } from './Note';

interface DragState {
  type: DragType;
  noteKey: string;
  originalBeat: number;
  originalNote: number;
  originalDuration: number;
  startX: number;
  startY: number;
}

export interface NoteCell {
  beat: number;
  note: number;
  velocity: number;
  duration: number;
  channel: number;
}

export interface NoteLayerProps {
  notes: NoteCell[];
  events: TimedFugueEvent[];
  mode: 'view' | 'edit';
  noteRange: { min: number; max: number };
  durationBeats: number;
  pixelsPerBeat: number;
  noteHeight: number;
  midiChannel: number;
  onEventsChange?: (events: TimedFugueEvent[]) => void;
}

// Snap beat to grid (16th notes)
const snapBeat = (beat: number) => Math.round(beat * 4) / 4;

export const NoteLayer: React.FC<NoteLayerProps> = ({
  notes,
  events,
  mode,
  noteRange,
  durationBeats,
  pixelsPerBeat,
  noteHeight,
  midiChannel,
  onEventsChange,
}) => {
  const [dragState, setDragState] = useState<DragState | null>(null);
  const noteCount = noteRange.max - noteRange.min + 1;

  // Update note in events array
  const updateNote = useCallback((
    originalBeat: number,
    originalNote: number,
    newBeat: number,
    newNote: number,
    newDuration: number,
    channel: number,
    velocity: number
  ) => {
    if (!onEventsChange) return;

    const filteredEvents = events.filter(e => {
      const ev = e.event;
      if (ev.type === 'note_on') {
        if (Math.abs(e.beat_offset - originalBeat) < 0.01 && ev.note === originalNote) {
          return false;
        }
      }
      if (ev.type === 'note_off') {
        if (ev.note === originalNote) {
          const noteOnBeat = events.find(e2 =>
            e2.event.type === 'note_on' &&
            Math.abs(e2.beat_offset - originalBeat) < 0.01 &&
            e2.event.note === originalNote
          );
          if (noteOnBeat && e.beat_offset > originalBeat) {
            return false;
          }
        }
      }
      return true;
    });

    const noteOn: TimedFugueEvent = {
      beat_offset: newBeat,
      event: { type: 'note_on', channel, note: newNote, velocity },
    };
    const noteOff: TimedFugueEvent = {
      beat_offset: newBeat + newDuration,
      event: { type: 'note_off', channel, note: newNote },
    };

    onEventsChange([...filteredEvents, noteOn, noteOff]);
  }, [events, onEventsChange]);

  // Handle drag start on a note
  const handleNoteDragStart = useCallback((
    cell: NoteCell,
    dragType: DragType,
    e: React.MouseEvent
  ) => {
    if (mode !== 'edit') return;
    e.stopPropagation();
    e.preventDefault();

    setDragState({
      type: dragType,
      noteKey: `${cell.note}-${cell.beat}`,
      originalBeat: cell.beat,
      originalNote: cell.note,
      originalDuration: cell.duration,
      startX: e.clientX,
      startY: e.clientY,
    });
  }, [mode]);

  // Handle drag move
  useEffect(() => {
    if (!dragState || mode !== 'edit') return;

    const handleMouseMove = (e: MouseEvent) => {
      if (!dragState) return;

      const deltaX = e.clientX - dragState.startX;
      const deltaY = e.clientY - dragState.startY;
      const beatDelta = deltaX / pixelsPerBeat;
      const noteDelta = -Math.round(deltaY / noteHeight);

      const cell = notes.find(n =>
        Math.abs(n.beat - dragState.originalBeat) < 0.01 &&
        n.note === dragState.originalNote
      );
      if (!cell) return;

      let newBeat = dragState.originalBeat;
      let newNote = dragState.originalNote;
      let newDuration = dragState.originalDuration;

      switch (dragState.type) {
        case 'move':
          newBeat = snapBeat(Math.max(0, dragState.originalBeat + beatDelta));
          newNote = Math.max(noteRange.min, Math.min(noteRange.max, dragState.originalNote + noteDelta));
          break;
        case 'resize-start':
          const startDelta = snapBeat(beatDelta);
          newBeat = Math.max(0, dragState.originalBeat + startDelta);
          newDuration = Math.max(0.25, dragState.originalDuration - startDelta);
          break;
        case 'resize-end':
          newDuration = Math.max(0.25, snapBeat(dragState.originalDuration + beatDelta));
          break;
      }

      if (newBeat + newDuration > durationBeats) {
        newDuration = durationBeats - newBeat;
      }

      updateNote(
        dragState.originalBeat,
        dragState.originalNote,
        newBeat,
        newNote,
        newDuration,
        cell.channel,
        cell.velocity
      );

      setDragState(prev => prev ? {
        ...prev,
        originalBeat: newBeat,
        originalNote: newNote,
        originalDuration: newDuration,
        startX: e.clientX,
        startY: e.clientY,
      } : null);
    };

    const handleMouseUp = () => {
      setDragState(null);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [dragState, mode, notes, noteRange, pixelsPerBeat, noteHeight, durationBeats, updateNote]);

  // Handle click on empty cell to add note
  const handleCellClick = useCallback((beat: number, note: number) => {
    if (mode !== 'edit' || !onEventsChange) return;

    const existingIndex = events.findIndex(e =>
      e.event.type === 'note_on' &&
      Math.abs(e.beat_offset - beat) < 0.125 &&
      e.event.note === note
    );

    if (existingIndex >= 0) {
      const newEvents = events.filter((e, i) => {
        if (i === existingIndex) return false;
        if (e.event.type === 'note_off' && e.event.note === note && e.beat_offset > beat) {
          return false;
        }
        return true;
      });
      onEventsChange(newEvents);
    } else {
      const noteOn: TimedFugueEvent = {
        beat_offset: beat,
        event: { type: 'note_on', channel: midiChannel, note, velocity: 100 },
      };
      const noteOff: TimedFugueEvent = {
        beat_offset: beat + 0.5,
        event: { type: 'note_off', channel: midiChannel, note },
      };
      onEventsChange([...events, noteOn, noteOff]);
    }
  }, [mode, events, onEventsChange, midiChannel]);

  // Render clickable cells for edit mode
  const renderEditCells = () => {
    if (mode !== 'edit') return null;

    const cells = [];
    const beatSnap = 0.25;
    const numCells = Math.ceil(durationBeats / beatSnap);

    for (let i = 0; i < numCells; i++) {
      for (let j = 0; j < noteCount; j++) {
        const beat = i * beatSnap;
        const note = noteRange.max - j;
        cells.push(
          <rect
            key={`edit-${beat}-${note}`}
            x={beat * pixelsPerBeat}
            y={j * noteHeight}
            width={beatSnap * pixelsPerBeat}
            height={noteHeight}
            className="edit-cell"
            onClick={() => handleCellClick(beat, note)}
          />
        );
      }
    }
    return cells;
  };

  return (
    <>
      {renderEditCells()}
      {notes.map((cell, i) => {
        const x = cell.beat * pixelsPerBeat;
        const y = (noteRange.max - cell.note) * noteHeight;
        const width = Math.max(cell.duration * pixelsPerBeat - 1, 4);
        const opacity = 0.4 + (cell.velocity / 127) * 0.6;
        const isDragging = dragState?.noteKey === `${cell.note}-${cell.beat}`;

        return (
          <Note
            key={`${cell.note}-${cell.beat}-${i}`}
            x={x}
            y={y}
            width={width}
            opacity={opacity}
            noteHeight={noteHeight}
            mode={mode}
            isDragging={isDragging}
            onDragStart={(type, e) => handleNoteDragStart(cell, type, e)}
          />
        );
      })}
    </>
  );
};

export default NoteLayer;
