/**
 * MIDI Mapping — dedicated tab for managing the instance's CC slots.
 *
 * Split out of the Settings tab. Everything here is slot-related: add/remove,
 * rename, renumber, and wiggle-to-identify for the user's DAW's MIDI-learn.
 * Values and CC numbers poll once per second so external changes (incoming
 * CC, MCP-driven edits) surface quickly.
 */

import React, { useCallback, useEffect, useState } from 'react';
import {
  addSlot,
  getSlots,
  removeSlot,
  renameSlotApi,
  setSlotCc,
  wiggleSlot,
} from '../api';
import type { SlotInfo } from '../types';
import './Settings.css';

export interface MidiMappingProps {
  /** Instance whose slots are shown. */
  instance: string;
}

export const MidiMapping: React.FC<MidiMappingProps> = ({ instance }) => {
  const [slots, setSlots] = useState<SlotInfo[]>([]);
  const [wigglingSlot, setWigglingSlot] = useState<number | null>(null);

  const fetchSlots = useCallback(async () => {
    try {
      const response = await getSlots(instance);
      setSlots(response.slots);
    } catch (e) {
      console.error('Failed to fetch slots:', e);
    }
  }, [instance]);

  useEffect(() => {
    fetchSlots();
    const interval = setInterval(fetchSlots, 1000);
    return () => clearInterval(interval);
  }, [fetchSlots]);

  const handleWiggle = useCallback(async (slotIndex: number) => {
    if (wigglingSlot !== null) return;
    setWigglingSlot(slotIndex);
    try {
      await wiggleSlot(slotIndex, instance);
    } catch (e) {
      console.error('Failed to wiggle:', e);
    }
    setTimeout(() => setWigglingSlot(null), 1100);
  }, [wigglingSlot, instance]);

  const handleAddSlot = useCallback(async () => {
    // Pick the next unused CC number in 1–127; wraps harmlessly on overflow.
    const used = new Set(slots.map(s => s.cc).filter((c): c is number => c !== null));
    let nextCc = 1;
    while (nextCc < 128 && used.has(nextCc)) nextCc += 1;
    if (nextCc === 128) nextCc = 1;
    try {
      await addSlot(nextCc, `CC${nextCc}`, instance);
      await fetchSlots();
    } catch (e) {
      console.error('Failed to add slot:', e);
    }
  }, [slots, instance, fetchSlots]);

  const handleRemoveSlot = useCallback(async (slotIndex: number) => {
    try {
      await removeSlot(slotIndex, instance);
      await fetchSlots();
    } catch (e) {
      console.error('Failed to remove slot:', e);
    }
  }, [instance, fetchSlots]);

  const handleRenameSlot = useCallback(async (slotIndex: number, name: string) => {
    try {
      await renameSlotApi(slotIndex, name, instance);
      await fetchSlots();
    } catch (e) {
      console.error('Failed to rename slot:', e);
    }
  }, [instance, fetchSlots]);

  const handleSetCc = useCallback(async (slotIndex: number, cc: number) => {
    if (cc < 0 || cc > 127) return;
    try {
      await setSlotCc(slotIndex, cc, instance);
      await fetchSlots();
    } catch (e) {
      console.error('Failed to set CC:', e);
    }
  }, [instance, fetchSlots]);

  return (
    <div className="settings-view">
      <h2>MIDI Mapping</h2>
      <p className="settings-description">
        MIDI CC mappings for this instance. Rename a slot to match what the CC
        drives on your synth, or change the CC number. Use "↔" to wiggle a
        CC so your synth's MIDI-learn picks it up.
      </p>
      <div className="slots-grid">
        {slots.map((slot) => (
          <SlotRow
            key={slot.index}
            slot={slot}
            wiggling={wigglingSlot === slot.index}
            wiggleDisabled={wigglingSlot !== null}
            onWiggle={() => handleWiggle(slot.index)}
            onRename={(name) => handleRenameSlot(slot.index, name)}
            onSetCc={(cc) => handleSetCc(slot.index, cc)}
            onRemove={() => handleRemoveSlot(slot.index)}
          />
        ))}
        <button className="slot-add-btn" onClick={handleAddSlot} title="Add a new CC slot">
          + Add CC
        </button>
      </div>
    </div>
  );
};

/**
 * One row in the CC-slots grid. Both `name` and `cc` are editable inline —
 * the input tracks its own draft state until blur/Enter, so re-renders
 * driven by the 1s slot-poll don't clobber what the user is typing.
 */
interface SlotRowProps {
  slot: SlotInfo;
  wiggling: boolean;
  wiggleDisabled: boolean;
  onWiggle: () => void;
  onRename: (name: string) => void;
  onSetCc: (cc: number) => void;
  onRemove: () => void;
}

const SlotRow: React.FC<SlotRowProps> = ({ slot, wiggling, wiggleDisabled, onWiggle, onRename, onSetCc, onRemove }) => {
  const [nameDraft, setNameDraft] = useState(slot.name);
  const [nameEditing, setNameEditing] = useState(false);
  const [ccDraft, setCcDraft] = useState(String(slot.cc ?? ''));
  const [ccEditing, setCcEditing] = useState(false);

  useEffect(() => {
    if (!nameEditing) setNameDraft(slot.name);
  }, [slot.name, nameEditing]);
  useEffect(() => {
    if (!ccEditing) setCcDraft(String(slot.cc ?? ''));
  }, [slot.cc, ccEditing]);

  const commitName = () => {
    setNameEditing(false);
    if (nameDraft !== slot.name) onRename(nameDraft);
  };
  const commitCc = () => {
    setCcEditing(false);
    const parsed = parseInt(ccDraft, 10);
    if (Number.isFinite(parsed) && parsed >= 0 && parsed <= 127 && parsed !== slot.cc) {
      onSetCc(parsed);
    } else if (!Number.isFinite(parsed)) {
      setCcDraft(String(slot.cc ?? ''));
    }
  };

  return (
    <div className={`slot-row ${wiggling ? 'wiggling' : ''}`}>
      <button
        className="slot-remove-btn"
        onClick={onRemove}
        title="Remove this slot"
      >
        ×
      </button>
      <div className="slot-cc-row">
        <span className="slot-cc-prefix">CC</span>
        <input
          className="slot-cc-input"
          type="number"
          min={0}
          max={127}
          value={ccDraft}
          onChange={(e) => setCcDraft(e.target.value)}
          onFocus={() => setCcEditing(true)}
          onBlur={commitCc}
          onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur(); }}
        />
      </div>
      <div className="slot-knob-wrap">
        <button
          className="slot-knob"
          onClick={(e) => {
            onWiggle();
            (e.currentTarget as HTMLButtonElement).blur();
          }}
          disabled={slot.cc === null || wiggleDisabled}
          title={slot.cc === null ? 'Set a CC first' : 'Click to wiggle — helps your synth MIDI-learn this CC'}
        >
          <span className="slot-knob-glyph">{wiggling ? '~' : '↔'}</span>
        </button>
      </div>
      <input
        className="slot-name-input"
        value={nameDraft}
        onChange={(e) => setNameDraft(e.target.value)}
        onFocus={() => setNameEditing(true)}
        onBlur={commitName}
        onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur(); }}
      />
    </div>
  );
};

export default MidiMapping;
