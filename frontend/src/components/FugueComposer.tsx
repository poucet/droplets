/**
 * FugueComposer - Wrapper for creating/editing fugues
 */

import React, { useState, useCallback } from 'react';
import { FugueGrid } from './FugueGrid';
import type { TimedFugueEvent, LoopMode, QuantizeMode, CancelMode } from '../types';
import './FugueComposer.css';

export interface FugueComposerProps {
  onQueue: (fugue: ComposerFugue) => void;
  onCancel: () => void;
  /** Optional fugue to edit - if provided, composer starts with these values */
  initialFugue?: {
    tag: string | null;
    events: TimedFugueEvent[];
    duration_beats: number;
    loop_mode: LoopMode;
    quantize: QuantizeMode;
    cancel_mode: CancelMode;
  };
}

export interface ComposerFugue {
  tag: string | null;
  events: TimedFugueEvent[];
  duration_beats: number;
  loop_mode: LoopMode;
  quantize: QuantizeMode;
  cancel_mode: CancelMode;
}

const DEFAULT_DURATION = 4;
const DEFAULT_NOTE_RANGE = { min: 48, max: 72 }; // C2 to C4 (DAW convention, C3=60)
const MIDI_CHANNELS = Array.from({ length: 16 }, (_, i) => i); // 0-15

// Helper to extract loop times from LoopMode
const getLoopTimes = (mode: LoopMode): number => {
  if (typeof mode === 'object' && 'times' in mode) return mode.times;
  return 2;
};

// Helper to get the simple loop mode string for UI
const getLoopModeType = (mode: LoopMode): 'once' | 'times' | 'forever' => {
  if (mode === 'once') return 'once';
  if (mode === 'forever') return 'forever';
  if (typeof mode === 'object' && 'times' in mode) return 'times';
  return 'once';
};

// Helper to get quantize mode string for UI select
const getQuantizeModeType = (mode: QuantizeMode): string => {
  if (mode === 'immediate') return 'immediate';
  if (mode === 'beat') return 'beat';
  if (mode === 'bar') return 'bar';
  if (typeof mode === 'object' && 'bars' in mode) return `bars${mode.bars}`;
  return 'bar';
};

// Helper to get cancel mode type for UI
const getCancelModeType = (mode: CancelMode): 'none' | 'tag' | 'all' => {
  if (mode === 'none') return 'none';
  if (mode === 'cancel_all') return 'all';
  if (typeof mode === 'object' && 'cancel_by_tag' in mode) return 'tag';
  return 'none';
};

export const FugueComposer: React.FC<FugueComposerProps> = ({
  onQueue,
  onCancel,
  initialFugue,
}) => {
  const [events, setEvents] = useState<TimedFugueEvent[]>(initialFugue?.events ?? []);
  const [durationBeats, setDurationBeats] = useState(initialFugue?.duration_beats ?? DEFAULT_DURATION);
  const [tag, setTag] = useState(initialFugue?.tag ?? '');
  const [loopModeType, setLoopModeType] = useState<'once' | 'times' | 'forever'>(
    initialFugue ? getLoopModeType(initialFugue.loop_mode) : 'forever'
  );
  const [loopTimes, setLoopTimes] = useState(initialFugue ? getLoopTimes(initialFugue.loop_mode) : 2);
  const [quantizeModeType, setQuantizeModeType] = useState(
    initialFugue ? getQuantizeModeType(initialFugue.quantize) : 'bar'
  );
  const [cancelModeType, setCancelModeType] = useState<'none' | 'tag' | 'all'>(
    initialFugue ? getCancelModeType(initialFugue.cancel_mode) : 'none'
  );
  const [midiChannel, setMidiChannel] = useState(0);

  const handleQueue = useCallback(() => {
    // Build LoopMode from UI state
    let loop_mode: LoopMode;
    if (loopModeType === 'once') {
      loop_mode = 'once';
    } else if (loopModeType === 'times') {
      loop_mode = { times: loopTimes };
    } else {
      loop_mode = 'forever';
    }

    // Build QuantizeMode from UI state
    let quantize: QuantizeMode;
    switch (quantizeModeType) {
      case 'immediate':
        quantize = 'immediate';
        break;
      case 'beat':
        quantize = 'beat';
        break;
      case 'bar':
        quantize = 'bar';
        break;
      case 'bars2':
        quantize = { bars: 2 };
        break;
      case 'bars4':
        quantize = { bars: 4 };
        break;
      case 'bars8':
        quantize = { bars: 8 };
        break;
      default:
        quantize = 'bar';
    }

    // Build CancelMode from UI state
    let cancel_mode: CancelMode;
    if (cancelModeType === 'none') {
      cancel_mode = 'none';
    } else if (cancelModeType === 'tag' && tag) {
      cancel_mode = { cancel_by_tag: tag };
    } else if (cancelModeType === 'all') {
      cancel_mode = 'cancel_all';
    } else {
      cancel_mode = 'none';
    }

    const fugue: ComposerFugue = {
      tag: tag || null,
      events,
      duration_beats: durationBeats,
      loop_mode,
      quantize,
      cancel_mode,
    };

    onQueue(fugue);
  }, [events, durationBeats, tag, loopModeType, loopTimes, quantizeModeType, cancelModeType, onQueue]);

  const handleClear = useCallback(() => {
    setEvents([]);
  }, []);

  // Count notes — each `timed_note` is one note; each `note_on` is
  // also one note (raw on/off pairs from imports).
  const eventCount = events.filter(
    e => e.event.type === 'timed_note' || e.event.type === 'note_on'
  ).length;

  return (
    <div className="fugue-composer">
      <div className="composer-header">
        <h3>{initialFugue ? 'Edit Fugue' : 'Compose Fugue'}</h3>
        <button className="cancel-btn" onClick={onCancel}>Close</button>
      </div>

      <div className="composer-controls">
        <div className="control-group">
          <label>Tag</label>
          <input
            type="text"
            value={tag}
            onChange={(e) => setTag(e.target.value)}
            placeholder="e.g., melody"
          />
        </div>

        <div className="control-group">
          <label>Duration (beats)</label>
          <input
            type="number"
            min={1}
            max={64}
            value={durationBeats}
            onChange={(e) => setDurationBeats(Math.max(1, parseInt(e.target.value) || 1))}
          />
        </div>

        <div className="control-group">
          <label>Loop</label>
          <select value={loopModeType} onChange={(e) => setLoopModeType(e.target.value as 'once' | 'times' | 'forever')}>
            <option value="once">Once</option>
            <option value="times">Times</option>
            <option value="forever">Forever</option>
          </select>
          {loopModeType === 'times' && (
            <input
              type="number"
              min={1}
              max={100}
              value={loopTimes}
              onChange={(e) => setLoopTimes(Math.max(1, parseInt(e.target.value) || 1))}
              className="loop-times-input"
            />
          )}
        </div>

        <div className="control-group">
          <label>Quantize</label>
          <select value={quantizeModeType} onChange={(e) => setQuantizeModeType(e.target.value)}>
            <option value="immediate">Immediate</option>
            <option value="beat">1 Beat</option>
            <option value="bar">1 Bar</option>
            <option value="bars2">2 Bars</option>
            <option value="bars4">4 Bars</option>
            <option value="bars8">8 Bars</option>
          </select>
        </div>

        <div className="control-group">
          <label>Cancel</label>
          <select value={cancelModeType} onChange={(e) => setCancelModeType(e.target.value as 'none' | 'tag' | 'all')}>
            <option value="none">None</option>
            <option value="tag">Same Tag</option>
            <option value="all">All</option>
          </select>
        </div>

        <div className="control-group">
          <label>MIDI Channel</label>
          <select value={midiChannel} onChange={(e) => setMidiChannel(parseInt(e.target.value))}>
            {MIDI_CHANNELS.map(ch => (
              <option key={ch} value={ch}>Ch {ch + 1}</option>
            ))}
          </select>
        </div>
      </div>

      <div className="composer-grid">
        <FugueGrid
          events={events}
          durationBeats={durationBeats}
          mode="edit"
          onEventsChange={setEvents}
          noteRange={DEFAULT_NOTE_RANGE}
          pixelsPerBeat={60}
          noteHeight={12}
          midiChannel={midiChannel}
        />
      </div>

      <div className="composer-footer">
        <span className="note-count">{eventCount} notes</span>
        <div className="footer-buttons">
          <button className="clear-btn" onClick={handleClear} disabled={events.length === 0}>
            Clear
          </button>
          <button className="queue-btn" onClick={handleQueue} disabled={events.length === 0}>
            Queue Fugue
          </button>
        </div>
      </div>
    </div>
  );
};

export default FugueComposer;
