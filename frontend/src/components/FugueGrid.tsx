/**
 * FugueGrid - Shared component for viewing and editing fugue events
 *
 * Renders:
 * - Note events as a piano roll style grid
 * - CC events as automation lanes with line graphs
 * - Playhead synced to transport position via direct DOM manipulation
 */

import React, { useMemo, useRef, useEffect } from 'react';
import type { TimedFugueEvent } from '../types';
import { NoteLayer, NoteCell } from './NoteLayer';
import { getTimingManager } from '../timing';
import './FugueGrid.css';

// Note names for display
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const getNoteName = (midi: number) => `${NOTE_NAMES[midi % 12]}${Math.floor(midi / 12) - 1}`;
const isBlackKey = (midi: number) => [1, 3, 6, 8, 10].includes(midi % 12);

export interface FugueGridProps {
  events: TimedFugueEvent[];
  durationBeats: number;

  // Playback - for client-side interpolated playhead
  // startBeat: the transport beat when this fugue started playing
  // When provided with showPlayhead=true, uses TimingManager for smooth animation
  startBeat?: number;
  showPlayhead?: boolean;

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

interface PerNotePoint {
  beat: number;
  value: number; // pressure 0-1 OR semitones
  note: number;
  channel: number;
}

/// Semitone range displayed on a bend lane. Bends beyond this are clamped
/// visually (the audio-thread still honors the real value).
const BEND_DISPLAY_RANGE = 12;

export const FugueGrid: React.FC<FugueGridProps> = ({
  events,
  durationBeats,
  startBeat,
  showPlayhead = false,
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
  // Ref for direct DOM manipulation of playhead
  const playheadRef = useRef<SVGLineElement>(null);
  // Parse events into notes, CC points, and per-note expression lanes.
  const {
    notes, ccPoints, pressurePoints, bendPoints,
    autoNoteRange, autoCCs, autoPressureKeys, autoBendKeys,
  } = useMemo(() => {
    const noteMap = new Map<string, NoteCell>();
    const ccMap = new Map<number, CCPoint[]>();
    // Per-note expression keyed by `${note}-${channel}` so multiple notes can
    // have independent lanes on the same instrument.
    const pressureMap = new Map<string, PerNotePoint[]>();
    const bendMap = new Map<string, PerNotePoint[]>();
    let minNote = 127;
    let maxNote = 0;
    const ccSet = new Set<number>();
    const pressureSet = new Set<string>();
    const bendSet = new Set<string>();

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
      } else if (event.type === 'per_note_pressure') {
        const key = `${event.note}-${event.channel}`;
        pressureSet.add(key);
        const list = pressureMap.get(key) || [];
        list.push({
          beat: beat_offset,
          value: event.pressure,
          note: event.note,
          channel: event.channel,
        });
        pressureMap.set(key, list);
      } else if (event.type === 'per_note_pitch_bend') {
        const key = `${event.note}-${event.channel}`;
        bendSet.add(key);
        const list = bendMap.get(key) || [];
        list.push({
          beat: beat_offset,
          value: event.semitones,
          note: event.note,
          channel: event.channel,
        });
        bendMap.set(key, list);
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

    // Sort each per-note lane's points by beat for the polyline.
    const sortLane = (m: Map<string, PerNotePoint[]>) => {
      m.forEach(arr => arr.sort((a, b) => a.beat - b.beat));
    };
    sortLane(pressureMap);
    sortLane(bendMap);

    return {
      notes: Array.from(noteMap.values()),
      ccPoints: ccMap,
      pressurePoints: pressureMap,
      bendPoints: bendMap,
      autoNoteRange: minNote <= maxNote ? { min: minNote, max: maxNote } : { min: 48, max: 72 },
      autoCCs: Array.from(ccSet).sort((a, b) => a - b),
      autoPressureKeys: Array.from(pressureSet).sort(),
      autoBendKeys: Array.from(bendSet).sort(),
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
  const totalPressureHeight = autoPressureKeys.length * ccLaneHeight;
  const totalBendHeight = autoBendKeys.length * ccLaneHeight;
  const totalExtraHeight = totalCCHeight + totalPressureHeight + totalBendHeight;

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

  // Render per-note pressure lanes. Y axis is 0.0 (bottom) to 1.0 (top).
  // Values are per-segment-expanded, so point density is high; drop the
  // per-point circles that CC uses for legibility.
  const renderPressureLanes = () => {
    return autoPressureKeys.map((key, laneIndex) => {
      const pts = pressurePoints.get(key) || [];
      const [note] = key.split('-').map(Number);
      const y = laneIndex * ccLaneHeight;

      const pathD = pts.length > 0
        ? pts.map((p, i) => {
            const x = p.beat * pixelsPerBeat;
            const clamped = Math.max(0, Math.min(1, p.value));
            const py = ccLaneHeight - clamped * (ccLaneHeight - 4) - 2;
            return `${i === 0 ? 'M' : 'L'} ${x} ${py}`;
          }).join(' ')
        : '';

      return (
        <g key={key} transform={`translate(0, ${y})`}>
          <rect x={0} y={0} width={gridWidth} height={ccLaneHeight} className="pressure-lane-bg" />
          <text x={4} y={14} className="pressure-label">♦ {getNoteName(note)} pressure</text>
          {pathD && <path d={pathD} className="pressure-line" fill="none" />}
        </g>
      );
    });
  };

  // Render per-note pitch-bend lanes. Y axis is semitones, clamped to
  // ±BEND_DISPLAY_RANGE for display (audio-thread honors the real value).
  // A horizontal center line marks zero bend.
  const renderBendLanes = () => {
    return autoBendKeys.map((key, laneIndex) => {
      const pts = bendPoints.get(key) || [];
      const [note] = key.split('-').map(Number);
      const y = laneIndex * ccLaneHeight;
      const centerY = ccLaneHeight / 2;

      const pathD = pts.length > 0
        ? pts.map((p, i) => {
            const x = p.beat * pixelsPerBeat;
            // Clamp to display range, then map to lane coords. Positive
            // semitones go UP, so invert.
            const clamped = Math.max(-BEND_DISPLAY_RANGE, Math.min(BEND_DISPLAY_RANGE, p.value));
            const normalized = clamped / BEND_DISPLAY_RANGE; // [-1, 1]
            const py = centerY - normalized * (centerY - 2);
            return `${i === 0 ? 'M' : 'L'} ${x} ${py}`;
          }).join(' ')
        : '';

      return (
        <g key={key} transform={`translate(0, ${y})`}>
          <rect x={0} y={0} width={gridWidth} height={ccLaneHeight} className="bend-lane-bg" />
          <line
            x1={0} y1={centerY}
            x2={gridWidth} y2={centerY}
            className="bend-center-line"
          />
          <text x={4} y={14} className="bend-label">↕ {getNoteName(note)} bend</text>
          {pathD && <path d={pathD} className="bend-line" fill="none" />}
        </g>
      );
    });
  };

  // Subscribe to TimingManager for smooth playhead animation via direct DOM manipulation
  useEffect(() => {
    if (!showPlayhead || startBeat === undefined) return;

    const timing = getTimingManager();

    const updatePlayhead = (currentBeat: number) => {
      const line = playheadRef.current;
      if (!line) return;

      // Calculate local beat position within this fugue
      // Account for DAW looping: if currentBeat < startBeat, the DAW has looped
      // and the fugue is still playing from where it was
      const transport = timing.getTransport();
      let localBeat = currentBeat - startBeat;

      // If DAW is looping and we appear to be before start, we're actually
      // in a later iteration. Calculate how many loop cycles have passed.
      if (localBeat < 0 && transport.is_looping) {
        const loopLength = transport.loop_end_beat - transport.loop_start_beat;
        if (loopLength > 0) {
          // The fugue started at startBeat, DAW looped back, so add loop length
          // to get the effective elapsed beats
          const loopsPassed = Math.ceil((startBeat - currentBeat) / loopLength);
          localBeat += loopsPassed * loopLength;
        }
      }

      // Still negative means fugue hasn't started yet
      if (localBeat < 0) {
        line.style.display = 'none';
        return;
      }

      // Wrap within duration for looping
      const wrappedBeat = ((localBeat % durationBeats) + durationBeats) % durationBeats;
      const x = wrappedBeat * pixelsPerBeat;

      line.style.display = '';
      line.setAttribute('x1', String(x));
      line.setAttribute('x2', String(x));

      // Update playing class based on transport state
      if (timing.isPlaying()) {
        line.classList.add('playing');
      } else {
        line.classList.remove('playing');
      }
    };

    return timing.subscribe(updatePlayhead);
  }, [showPlayhead, startBeat, durationBeats, pixelsPerBeat]);

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
        {autoPressureKeys.length > 0 && (
          <div className="pressure-sidebar" style={{ height: totalPressureHeight }}>
            {autoPressureKeys.map(key => {
              const [note] = key.split('-').map(Number);
              return (
                <div key={key} className="pressure-sidebar-label" style={{ height: ccLaneHeight }}>
                  ♦ {getNoteName(note)}
                </div>
              );
            })}
          </div>
        )}
        {autoBendKeys.length > 0 && (
          <div className="bend-sidebar" style={{ height: totalBendHeight }}>
            {autoBendKeys.map(key => {
              const [note] = key.split('-').map(Number);
              return (
                <div key={key} className="bend-sidebar-label" style={{ height: ccLaneHeight }}>
                  ↕ {getNoteName(note)}
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div className="grid-scroll">
        <svg
          width={gridWidth}
          height={noteGridHeight + totalExtraHeight}
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

          {/* Per-note pressure lanes */}
          <g className="pressure-lanes" transform={`translate(0, ${noteGridHeight + totalCCHeight})`}>
            {renderPressureLanes()}
          </g>

          {/* Per-note pitch-bend lanes */}
          <g className="bend-lanes" transform={`translate(0, ${noteGridHeight + totalCCHeight + totalPressureHeight})`}>
            {renderBendLanes()}
          </g>

          {/* Playhead (on top of everything) - positioned via direct DOM manipulation */}
          {showPlayhead && startBeat !== undefined && (
            <line
              ref={playheadRef}
              x1={0}
              y1={0}
              x2={0}
              y2={noteGridHeight + totalExtraHeight}
              className="playhead"
              style={{ display: 'none' }}
            />
          )}
        </svg>
      </div>
    </div>
  );
};

export default FugueGrid;
