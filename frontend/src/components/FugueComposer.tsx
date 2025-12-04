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
const DEFAULT_NOTE_RANGE = { min: 48, max: 72 }; // C3 to C5
const MIDI_CHANNELS = Array.from({ length: 16 }, (_, i) => i); // 0-15

// Helper to convert LoopMode to internal state
const parseLoopMode = (mode: LoopMode): { loopMode: 'once' | 'times' | 'forever'; loopTimes: number } => {
  if (mode === 'Once') return { loopMode: 'once', loopTimes: 2 };
  if (mode === 'Forever') return { loopMode: 'forever', loopTimes: 2 };
  if (typeof mode === 'object' && 'Times' in mode) return { loopMode: 'times', loopTimes: mode.Times };
  return { loopMode: 'once', loopTimes: 2 };
};

// Helper to convert QuantizeMode to internal state
const parseQuantizeMode = (mode: QuantizeMode): 'immediate' | 'beat' | 'bar' | 'bars2' | 'bars4' | 'bars8' => {
  if (mode === 'Immediate') return 'immediate';
  if (mode === 'Beat') return 'beat';
  if (mode === 'Bar') return 'bar';
  if (typeof mode === 'object' && 'Bars' in mode) {
    if (mode.Bars === 2) return 'bars2';
    if (mode.Bars === 4) return 'bars4';
    if (mode.Bars === 8) return 'bars8';
  }
  return 'bar';
};

// Helper to convert CancelMode to internal state
const parseCancelMode = (mode: CancelMode): 'none' | 'tag' | 'all' => {
  if (mode === 'None') return 'none';
  if (mode === 'CancelAll') return 'all';
  if (typeof mode === 'object' && 'CancelByTag' in mode) return 'tag';
  return 'none';
};

export const FugueComposer: React.FC<FugueComposerProps> = ({
  onQueue,
  onCancel,
  initialFugue,
}) => {
  // Parse initial values from initialFugue if provided
  const initialLoopState = initialFugue ? parseLoopMode(initialFugue.loop_mode) : { loopMode: 'once' as const, loopTimes: 2 };
  const initialQuantize = initialFugue ? parseQuantizeMode(initialFugue.quantize) : 'bar';
  const initialCancelMode = initialFugue ? parseCancelMode(initialFugue.cancel_mode) : 'none';

  const [events, setEvents] = useState<TimedFugueEvent[]>(initialFugue?.events ?? []);
  const [durationBeats, setDurationBeats] = useState(initialFugue?.duration_beats ?? DEFAULT_DURATION);
  const [tag, setTag] = useState(initialFugue?.tag ?? '');
  const [loopMode, setLoopMode] = useState<'once' | 'times' | 'forever'>(initialLoopState.loopMode);
  const [loopTimes, setLoopTimes] = useState(initialLoopState.loopTimes);
  const [quantize, setQuantize] = useState<'immediate' | 'beat' | 'bar' | 'bars2' | 'bars4' | 'bars8'>(initialQuantize);
  const [cancelMode, setCancelMode] = useState<'none' | 'tag' | 'all'>(initialCancelMode);
  const [midiChannel, setMidiChannel] = useState(0);

  const handleQueue = useCallback(() => {
    // Build the fugue definition
    let loop_mode: LoopMode;
    if (loopMode === 'once') {
      loop_mode = 'Once';
    } else if (loopMode === 'times') {
      loop_mode = { Times: loopTimes };
    } else {
      loop_mode = 'Forever';
    }

    let quantize_mode: QuantizeMode;
    switch (quantize) {
      case 'immediate':
        quantize_mode = 'Immediate';
        break;
      case 'beat':
        quantize_mode = 'Beat';
        break;
      case 'bar':
        quantize_mode = 'Bar';
        break;
      case 'bars2':
        quantize_mode = { Bars: 2 };
        break;
      case 'bars4':
        quantize_mode = { Bars: 4 };
        break;
      case 'bars8':
        quantize_mode = { Bars: 8 };
        break;
      default:
        quantize_mode = 'Bar';
    }

    let cancel_mode: CancelMode;
    if (cancelMode === 'none') {
      cancel_mode = 'None';
    } else if (cancelMode === 'tag' && tag) {
      cancel_mode = { CancelByTag: tag };
    } else if (cancelMode === 'all') {
      cancel_mode = 'CancelAll';
    } else {
      cancel_mode = 'None';
    }

    const fugue: ComposerFugue = {
      tag: tag || null,
      events,
      duration_beats: durationBeats,
      loop_mode,
      quantize: quantize_mode,
      cancel_mode,
    };

    onQueue(fugue);
  }, [events, durationBeats, tag, loopMode, loopTimes, quantize, cancelMode, onQueue]);

  const handleClear = useCallback(() => {
    setEvents([]);
  }, []);

  const eventCount = events.filter(e => {
    const ev = e.event;
    return 'NoteOn' in ev || (ev as any).type === 'note_on';
  }).length;

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
          <select value={loopMode} onChange={(e) => setLoopMode(e.target.value as 'once' | 'times' | 'forever')}>
            <option value="once">Once</option>
            <option value="times">Times</option>
            <option value="forever">Forever</option>
          </select>
          {loopMode === 'times' && (
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
          <select value={quantize} onChange={(e) => setQuantize(e.target.value as 'immediate' | 'beat' | 'bar' | 'bars2' | 'bars4' | 'bars8')}>
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
          <select value={cancelMode} onChange={(e) => setCancelMode(e.target.value as 'none' | 'tag' | 'all')}>
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
