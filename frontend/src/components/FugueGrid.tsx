/**
 * FugueGrid - Shared component for viewing and editing fugue events
 *
 * Renders:
 * - Note events as a piano roll style grid
 * - CC events as automation lanes with line graphs
 * - Playhead synced to transport position
 */

import React, { useMemo } from 'react';
import type { TimedFugueEvent } from '../types';
import { NoteLayer, NoteCell } from './NoteLayer';
import './FugueGrid.css';

// Note names for display
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const getNoteName = (midi: number) => `${NOTE_NAMES[midi % 12]}${Math.floor(midi / 12) - 1}`;
const isBlackKey = (midi: number) => [1, 3, 6, 8, 10].includes(midi % 12);

export interface FugueGridProps {
  events: TimedFugueEvent[];
  durationBeats: number;

  // Playback
  playheadBeat?: number;
  isPlaying?: boolean;

  // Display options
  mode: 'view' | 'edit';
  noteRange?: { min: number; max: number };
  visibleCCs?: number[];
  beatsPerBar?: number;

  // Edit mode callbacks
  onEventsChange?: (events: TimedFugueEvent[]) => void;

  // Edit mode options
  midiChannel?: number;

  // Sizing
  pixelsPerBeat?: number;
  noteHeight?: number;
  ccLaneHeight?: number;
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
    const noteMap = new Map<string, NoteCell>();
    const ccMap = new Map<number, CCPoint[]>();
    let minNote = 127;
    let maxNote = 0;
    const ccSet = new Set<number>();

    const activeNotes = new Map<number, { beat: number; velocity: number; channel: number }>();
    const sortedEvents = [...events].sort((a, b) => a.beat_offset - b.beat_offset);

    for (const { beat_offset, event } of sortedEvents) {
      if (event.type === 'note_on') {
        activeNotes.set(event.note, { beat: beat_offset, velocity: event.velocity, channel: event.channel });
        minNote = Math.min(minNote, event.note);
        maxNote = Math.max(maxNote, event.note);
      } else if (event.type === 'note_off') {
        const start = activeNotes.get(event.note);
        if (start) {
          const duration = beat_offset - start.beat;
          const key = `${event.note}-${start.beat}`;
          noteMap.set(key, {
            beat: start.beat,
            note: event.note,
            velocity: start.velocity,
            duration: Math.max(duration, 0.25),
            channel: start.channel,
          });
          activeNotes.delete(event.note);
        }
      } else if (event.type === 'cc') {
        ccSet.add(event.cc);
        const points = ccMap.get(event.cc) || [];
        points.push({ beat: beat_offset, cc: event.cc, value: event.value });
        ccMap.set(event.cc, points);
      }
    }

    // Handle notes without explicit note-off
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

  // Render note rows (background)
  const renderNoteRows = () => {
    const rows = [];
    for (let i = 0; i < noteCount; i++) {
      const note = noteRange.max - i;
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

  // Render piano roll sidebar
  const renderPianoKeys = () => {
    const keys = [];
    for (let i = 0; i < noteCount; i++) {
      const note = noteRange.max - i;
      const isBlack = isBlackKey(note);
      const showLabel = note % 12 === 0;
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

    // playheadBeat is already wrapped by the caller, just clamp to valid range
    const clampedBeat = Math.max(0, Math.min(playheadBeat, durationBeats));
    const x = clampedBeat * pixelsPerBeat;

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
          width={gridWidth}
          height={noteGridHeight + totalCCHeight}
          className="grid-svg"
        >
          {/* Note grid area */}
          <g className="note-grid">
            {renderNoteRows()}
            {renderGridLines()}
            <NoteLayer
              notes={notes}
              events={events}
              mode={mode}
              noteRange={noteRange}
              durationBeats={durationBeats}
              pixelsPerBeat={pixelsPerBeat}
              noteHeight={noteHeight}
              midiChannel={midiChannel}
              onEventsChange={onEventsChange}
            />
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
