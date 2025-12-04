/**
 * FugueGrid - Shared component for viewing and editing fugue events
 *
 * Renders:
 * - Note events as a piano roll style grid
 * - CC events as automation lanes with line graphs
 * - Playhead synced to transport position
 */

import React, { useMemo, useCallback, useState, useRef, useEffect } from 'react';
import type { TimedFugueEvent, FugueEvent } from '../types';
import './FugueGrid.css';

// Drag operation types
type DragType = 'move' | 'resize-start' | 'resize-end';

interface DragState {
  type: DragType;
  noteKey: string;           // `${note}-${beat}` identifier
  originalBeat: number;
  originalNote: number;
  originalDuration: number;
  startX: number;
  startY: number;
}

// Note names for display
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const getNoteName = (midi: number) => `${NOTE_NAMES[midi % 12]}${Math.floor(midi / 12) - 1}`;
const isBlackKey = (midi: number) => [1, 3, 6, 8, 10].includes(midi % 12);

export interface FugueGridProps {
  events: TimedFugueEvent[];
  durationBeats: number;

  // Playback
  playheadBeat?: number;        // undefined = no playhead shown
  isPlaying?: boolean;          // affects playhead animation style

  // Display options
  mode: 'view' | 'edit';
  noteRange?: { min: number; max: number };  // default: auto from events or 48-72
  visibleCCs?: number[];        // which CC lanes to show (default: auto from events)
  beatsPerBar?: number;         // for grid lines (default: 4)

  // Edit mode callbacks
  onEventsChange?: (events: TimedFugueEvent[]) => void;

  // Edit mode options
  midiChannel?: number;         // MIDI channel for new notes (default: 0)

  // Sizing
  pixelsPerBeat?: number;       // horizontal zoom (default: 40)
  noteHeight?: number;          // vertical note size (default: 16)
  ccLaneHeight?: number;        // CC automation lane height (default: 60)
}

interface NoteCell {
  beat: number;
  note: number;
  velocity: number;
  duration: number; // in beats
  channel: number;
}

interface CCPoint {
  beat: number;
  cc: number;
  value: number;
}

export const FugueGrid: React.FC<FugueGridProps> = ({
  events,
  durationBeats,
  playheadBeat,
  isPlaying = false,
  mode,
  noteRange: noteRangeProp,
  visibleCCs: visibleCCsProp,
  beatsPerBar = 4,
  onEventsChange,
  midiChannel = 0,
  pixelsPerBeat = 40,
  noteHeight = 16,
  ccLaneHeight = 60,
}) => {
  // Parse events into notes and CC points
  const { notes, ccPoints, autoNoteRange, autoCCs } = useMemo(() => {
    const noteMap = new Map<string, NoteCell>(); // key: `${note}-${beat}`
    const ccMap = new Map<number, CCPoint[]>(); // key: cc number
    let minNote = 127;
    let maxNote = 0;
    const ccSet = new Set<number>();

    // Track active notes for calculating duration
    const activeNotes = new Map<number, { beat: number; velocity: number; channel: number }>();

    // Sort events by beat
    const sortedEvents = [...events].sort((a, b) => a.beat_offset - b.beat_offset);

    for (const { beat_offset, event } of sortedEvents) {
      if ('NoteOn' in event || (event as any).type === 'note_on') {
        const noteOn = 'NoteOn' in event ? event.NoteOn : event as any;
        const note = noteOn.note;
        const velocity = noteOn.velocity;
        const channel = noteOn.channel ?? 0;

        activeNotes.set(note, { beat: beat_offset, velocity, channel });
        minNote = Math.min(minNote, note);
        maxNote = Math.max(maxNote, note);
      } else if ('NoteOff' in event || (event as any).type === 'note_off') {
        const noteOff = 'NoteOff' in event ? event.NoteOff : event as any;
        const note = noteOff.note;

        const start = activeNotes.get(note);
        if (start) {
          const duration = beat_offset - start.beat;
          const key = `${note}-${start.beat}`;
          noteMap.set(key, {
            beat: start.beat,
            note,
            velocity: start.velocity,
            duration: Math.max(duration, 0.25), // minimum duration
            channel: start.channel,
          });
          activeNotes.delete(note);
        }
      } else if ('Cc' in event || (event as any).type === 'cc') {
        const cc = 'Cc' in event ? event.Cc : event as any;
        ccSet.add(cc.cc);
        const points = ccMap.get(cc.cc) || [];
        points.push({ beat: beat_offset, cc: cc.cc, value: cc.value });
        ccMap.set(cc.cc, points);
      }
    }

    // Handle notes that don't have explicit note-off
    Array.from(activeNotes.entries()).forEach(([note, start]) => {
      const key = `${note}-${start.beat}`;
      noteMap.set(key, {
        beat: start.beat,
        note,
        velocity: start.velocity,
        duration: durationBeats - start.beat,
        channel: start.channel,
      });
    });

    return {
      notes: Array.from(noteMap.values()),
      ccPoints: ccMap,
      autoNoteRange: minNote <= maxNote ? { min: minNote, max: maxNote } : { min: 48, max: 72 },
      autoCCs: Array.from(ccSet).sort((a, b) => a - b),
    };
  }, [events, durationBeats]);

  // Effective note range
  const noteRange = noteRangeProp || {
    min: Math.max(0, autoNoteRange.min - 2),
    max: Math.min(127, autoNoteRange.max + 2),
  };
  const noteCount = noteRange.max - noteRange.min + 1;

  // Effective CC list
  const visibleCCs = visibleCCsProp || autoCCs;

  // Grid dimensions
  const gridWidth = durationBeats * pixelsPerBeat;
  const noteGridHeight = noteCount * noteHeight;
  const totalCCHeight = visibleCCs.length * ccLaneHeight;

  // Drag state for note manipulation
  const [dragState, setDragState] = useState<DragState | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  // Snap beat to grid (16th notes)
  const snapBeat = (beat: number) => Math.round(beat * 4) / 4;

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

    // Remove the original note events
    const filteredEvents = events.filter(e => {
      const ev = e.event;
      if ('NoteOn' in ev || (ev as any).type === 'note_on') {
        const noteOn = 'NoteOn' in ev ? ev.NoteOn : ev as any;
        if (Math.abs(e.beat_offset - originalBeat) < 0.01 && noteOn.note === originalNote) {
          return false;
        }
      }
      if ('NoteOff' in ev || (ev as any).type === 'note_off') {
        const noteOff = 'NoteOff' in ev ? ev.NoteOff : ev as any;
        if (noteOff.note === originalNote) {
          // Find if this note-off corresponds to our note-on
          const noteOnBeat = events.find(e2 => {
            const ev2 = e2.event;
            if ('NoteOn' in ev2 || (ev2 as any).type === 'note_on') {
              const noteOn2 = 'NoteOn' in ev2 ? ev2.NoteOn : ev2 as any;
              return Math.abs(e2.beat_offset - originalBeat) < 0.01 && noteOn2.note === originalNote;
            }
            return false;
          });
          if (noteOnBeat && e.beat_offset > originalBeat) {
            return false;
          }
        }
      }
      return true;
    });

    // Add the updated note
    const noteOn: TimedFugueEvent = {
      beat_offset: newBeat,
      event: { type: 'note_on', channel, note: newNote, velocity } as any,
    };
    const noteOff: TimedFugueEvent = {
      beat_offset: newBeat + newDuration,
      event: { type: 'note_off', channel, note: newNote } as any,
    };

    onEventsChange([...filteredEvents, noteOn, noteOff]);
  }, [events, onEventsChange]);

  // Handle drag start on a note
  const handleNoteDragStart = useCallback((
    e: React.MouseEvent,
    cell: NoteCell,
    dragType: DragType
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
      if (!svgRef.current || !dragState) return;

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

      // Ensure note doesn't extend past duration
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

      // Update drag state to track from new position
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

  // Handle note cell click in edit mode
  const handleNoteClick = useCallback((beat: number, note: number) => {
    if (mode !== 'edit' || !onEventsChange) return;

    // Find if there's already a note at this position
    const existingIndex = events.findIndex(e => {
      const ev = e.event;
      if ('NoteOn' in ev || (ev as any).type === 'note_on') {
        const noteOn = 'NoteOn' in ev ? ev.NoteOn : ev as any;
        return Math.abs(e.beat_offset - beat) < 0.125 && noteOn.note === note;
      }
      return false;
    });

    if (existingIndex >= 0) {
      // Remove note (and its note-off)
      const newEvents = events.filter((e, i) => {
        if (i === existingIndex) return false;
        // Also remove corresponding note-off
        const ev = e.event;
        if ('NoteOff' in ev || (ev as any).type === 'note_off') {
          const noteOff = 'NoteOff' in ev ? ev.NoteOff : ev as any;
          if (noteOff.note === note && e.beat_offset > beat) return false;
        }
        return true;
      });
      onEventsChange(newEvents);
    } else {
      // Add new note (default 0.5 beat duration)
      const noteOn: TimedFugueEvent = {
        beat_offset: beat,
        event: { type: 'note_on', channel: midiChannel, note, velocity: 100 } as any,
      };
      const noteOff: TimedFugueEvent = {
        beat_offset: beat + 0.5,
        event: { type: 'note_off', channel: midiChannel, note } as any,
      };
      onEventsChange([...events, noteOn, noteOff]);
    }
  }, [mode, events, onEventsChange, midiChannel]);

  // Render beat grid lines
  const renderGridLines = () => {
    const lines = [];
    for (let beat = 0; beat <= durationBeats; beat++) {
      const x = beat * pixelsPerBeat;
      const isBar = beat % beatsPerBar === 0;
      lines.push(
        <line
          key={beat}
          x1={x}
          y1={0}
          x2={x}
          y2={noteGridHeight}
          className={`grid-line ${isBar ? 'bar' : 'beat'}`}
        />
      );
    }
    return lines;
  };

  // Render note rows
  const renderNoteRows = () => {
    const rows = [];
    for (let i = 0; i < noteCount; i++) {
      const note = noteRange.max - i; // Top to bottom = high to low
      const y = i * noteHeight;
      const isBlack = isBlackKey(note);
      rows.push(
        <rect
          key={note}
          x={0}
          y={y}
          width={gridWidth}
          height={noteHeight}
          className={`note-row ${isBlack ? 'black' : 'white'}`}
        />
      );
    }
    return rows;
  };

  // Render note cells (the actual notes) with drag handles in edit mode
  const renderNotes = () => {
    const handleWidth = 6; // Width of resize handles

    return notes.map((cell, i) => {
      const x = cell.beat * pixelsPerBeat;
      const y = (noteRange.max - cell.note) * noteHeight;
      const width = Math.max(cell.duration * pixelsPerBeat - 1, 4);
      const opacity = 0.4 + (cell.velocity / 127) * 0.6;
      const isDragging = dragState?.noteKey === `${cell.note}-${cell.beat}`;

      if (mode === 'edit') {
        return (
          <g key={`${cell.note}-${cell.beat}-${i}`} className={`note-group ${isDragging ? 'dragging' : ''}`}>
            {/* Main note body - drag to move */}
            <rect
              x={x + handleWidth}
              y={y + 1}
              width={Math.max(width - handleWidth * 2, 2)}
              height={noteHeight - 2}
              className="note-cell note-body"
              style={{ opacity }}
              onMouseDown={(e) => handleNoteDragStart(e, cell, 'move')}
            />
            {/* Left resize handle */}
            <rect
              x={x}
              y={y + 1}
              width={handleWidth}
              height={noteHeight - 2}
              rx={2}
              className="note-cell note-handle note-handle-start"
              style={{ opacity }}
              onMouseDown={(e) => handleNoteDragStart(e, cell, 'resize-start')}
            />
            {/* Right resize handle */}
            <rect
              x={x + width - handleWidth}
              y={y + 1}
              width={handleWidth}
              height={noteHeight - 2}
              rx={2}
              className="note-cell note-handle note-handle-end"
              style={{ opacity }}
              onMouseDown={(e) => handleNoteDragStart(e, cell, 'resize-end')}
            />
          </g>
        );
      }

      // View mode - simple rectangle
      return (
        <rect
          key={`${cell.note}-${cell.beat}-${i}`}
          x={x}
          y={y + 1}
          width={width}
          height={noteHeight - 2}
          rx={2}
          className="note-cell"
          style={{ opacity }}
        />
      );
    });
  };

  // Render clickable cells for edit mode
  const renderEditCells = () => {
    if (mode !== 'edit') return null;

    const cells = [];
    const beatSnap = 0.25; // Snap to 16th notes
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
            onClick={() => handleNoteClick(beat, note)}
          />
        );
      }
    }
    return cells;
  };

  // Render piano roll sidebar
  const renderPianoKeys = () => {
    const keys = [];
    for (let i = 0; i < noteCount; i++) {
      const note = noteRange.max - i;
      const isBlack = isBlackKey(note);
      const showLabel = note % 12 === 0; // Show C notes
      keys.push(
        <div
          key={note}
          className={`piano-key ${isBlack ? 'black' : 'white'}`}
          style={{ height: noteHeight }}
        >
          {showLabel && <span className="key-label">{getNoteName(note)}</span>}
        </div>
      );
    }
    return keys;
  };

  // Render CC lanes
  const renderCCLanes = () => {
    return visibleCCs.map((cc, laneIndex) => {
      const points = ccPoints.get(cc) || [];
      const y = laneIndex * ccLaneHeight;

      // Build SVG path for CC line
      let pathD = '';
      if (points.length > 0) {
        const sorted = [...points].sort((a, b) => a.beat - b.beat);
        pathD = sorted.map((p, i) => {
          const x = p.beat * pixelsPerBeat;
          const py = ccLaneHeight - (p.value / 127) * (ccLaneHeight - 4) - 2;
          return `${i === 0 ? 'M' : 'L'} ${x} ${py}`;
        }).join(' ');
      }

      return (
        <g key={cc} transform={`translate(0, ${y})`}>
          <rect
            x={0}
            y={0}
            width={gridWidth}
            height={ccLaneHeight}
            className="cc-lane-bg"
          />
          <text x={4} y={14} className="cc-label">CC{cc}</text>
          {pathD && (
            <path d={pathD} className="cc-line" fill="none" />
          )}
          {points.map((p, i) => (
            <circle
              key={i}
              cx={p.beat * pixelsPerBeat}
              cy={ccLaneHeight - (p.value / 127) * (ccLaneHeight - 4) - 2}
              r={3}
              className="cc-point"
            />
          ))}
        </g>
      );
    });
  };

  // Render playhead
  const renderPlayhead = () => {
    if (playheadBeat === undefined) return null;

    // Handle looping - wrap playhead within duration
    const wrappedBeat = playheadBeat % durationBeats;
    const x = wrappedBeat * pixelsPerBeat;

    return (
      <line
        x1={x}
        y1={0}
        x2={x}
        y2={noteGridHeight + totalCCHeight}
        className={`playhead ${isPlaying ? 'playing' : ''}`}
      />
    );
  };

  return (
    <div className={`fugue-grid ${mode}`}>
      <div className="piano-sidebar">
        {renderPianoKeys()}
        {/* Spacer for CC lanes */}
        {visibleCCs.length > 0 && (
          <div className="cc-sidebar" style={{ height: totalCCHeight }}>
            {visibleCCs.map(cc => (
              <div key={cc} className="cc-sidebar-label" style={{ height: ccLaneHeight }}>
                CC{cc}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="grid-scroll">
        <svg
          ref={svgRef}
          width={gridWidth}
          height={noteGridHeight + totalCCHeight}
          className="grid-svg"
        >
          {/* Note grid area */}
          <g className="note-grid">
            {renderNoteRows()}
            {renderGridLines()}
            {mode === 'edit' && renderEditCells()}
            {renderNotes()}
          </g>

          {/* CC lanes */}
          <g className="cc-lanes" transform={`translate(0, ${noteGridHeight})`}>
            {renderCCLanes()}
          </g>

          {/* Playhead (on top of everything) */}
          {renderPlayhead()}
        </svg>
      </div>
    </div>
  );
};

export default FugueGrid;
