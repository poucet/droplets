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

export const FugueComposer: React.FC<FugueComposerProps> = ({
  onQueue,
  onCancel,
}) => {
  const [events, setEvents] = useState<TimedFugueEvent[]>([]);
  const [durationBeats, setDurationBeats] = useState(DEFAULT_DURATION);
  const [tag, setTag] = useState('');
  const [loopMode, setLoopMode] = useState<'once' | 'times' | 'forever'>('once');
  const [loopTimes, setLoopTimes] = useState(2);
  const [quantize, setQuantize] = useState<'immediate' | 'beat' | 'bar'>('bar');
  const [cancelMode, setCancelMode] = useState<'none' | 'tag' | 'all'>('none');

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
    if (quantize === 'immediate') {
      quantize_mode = 'Immediate';
    } else if (quantize === 'beat') {
      quantize_mode = 'NextBeat';
    } else {
      quantize_mode = 'NextBar';
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
        <h3>Compose Fugue</h3>
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
          <select value={quantize} onChange={(e) => setQuantize(e.target.value as 'immediate' | 'beat' | 'bar')}>
            <option value="immediate">Immediate</option>
            <option value="beat">Next Beat</option>
            <option value="bar">Next Bar</option>
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
