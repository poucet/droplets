/**
 * Settings - Configuration view for export path and MCP server info
 */

import React, { useEffect, useState, useCallback } from 'react';
import {
  getSettings,
  updateSettings,
  revealExports,
  getSlots,
  wiggleSlot,
  addSlot,
  removeSlot,
  setSlotCc,
  renameSlotApi,
  type SettingsResponse,
} from '../api';
import type { SlotInfo } from '../types';
import './Settings.css';

export interface SettingsProps {
  /** Instance whose slot map is shown in the "Parameter Slots" section. */
  instance: string;
}

export const Settings: React.FC<SettingsProps> = ({ instance }) => {
  const [settings, setSettings] = useState<SettingsResponse | null>(null);
  const [exportPath, setExportPath] = useState('');
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  // Parameter slots for the active instance — moved in from the old Monitor
  // tab so CC mappings sit next to the settings that configure them. Polled
  // every second to reflect external CC input (MIDI learn, knob wiggles).
  const [slots, setSlots] = useState<SlotInfo[]>([]);
  const [wigglingSlot, setWigglingSlot] = useState<number | null>(null);

  // Custom instructions are appended to the MCP system prompt — give the
  // LLM per-setup context like synth CC mappings or stylistic rules.
  const [customInstructions, setCustomInstructions] = useState('');
  const [isInstructionsDirty, setIsInstructionsDirty] = useState(false);
  const [isSavingInstructions, setIsSavingInstructions] = useState(false);

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
    // Pick the next unused CC number in the 1–127 range; fall back to 1 if
    // everything's taken (wraparound is user-visible but harmless).
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

  const fetchSettings = useCallback(async () => {
    try {
      const response = await getSettings();
      setSettings(response);
      setExportPath(response.export_path);
      setCustomInstructions(response.custom_instructions);
      setIsInstructionsDirty(false);
    } catch (e) {
      console.error('Failed to fetch settings:', e);
      setError('Failed to load settings');
    }
  }, []);

  useEffect(() => {
    fetchSettings();
  }, [fetchSettings]);

  const handleSave = useCallback(async () => {
    setIsSaving(true);
    setError(null);
    setSuccess(null);

    try {
      await updateSettings({ export_path: exportPath });
      setSuccess('Settings saved');
      setIsEditing(false);
      await fetchSettings();
    } catch (e) {
      console.error('Failed to save settings:', e);
      setError('Failed to save settings');
    } finally {
      setIsSaving(false);
    }
  }, [exportPath, fetchSettings]);

  const handleReset = useCallback(() => {
    if (settings) {
      setExportPath(settings.export_path);
      setIsEditing(false);
    }
  }, [settings]);

  const handleSaveInstructions = useCallback(async () => {
    setIsSavingInstructions(true);
    setError(null);
    setSuccess(null);
    try {
      await updateSettings({ custom_instructions: customInstructions });
      setSuccess('Custom instructions saved — reconnect Claude to pick them up');
      setIsInstructionsDirty(false);
      await fetchSettings();
      setTimeout(() => setSuccess(null), 4000);
    } catch (e) {
      console.error('Failed to save instructions:', e);
      setError('Failed to save custom instructions');
    } finally {
      setIsSavingInstructions(false);
    }
  }, [customInstructions, fetchSettings]);

  const handleResetInstructions = useCallback(() => {
    if (settings) {
      setCustomInstructions(settings.custom_instructions);
      setIsInstructionsDirty(false);
    }
  }, [settings]);

  const handleRevealExports = useCallback(async () => {
    try {
      await revealExports();
    } catch (e) {
      console.error('Failed to open exports folder:', e);
      setError('Failed to open exports folder');
    }
  }, []);

  const handleCopyUrl = useCallback(async () => {
    if (settings) {
      try {
        await navigator.clipboard.writeText(settings.mcp_url);
        setSuccess('URL copied to clipboard');
      } catch {
        // Fallback for non-secure contexts
        const textArea = document.createElement('textarea');
        textArea.value = settings.mcp_url;
        textArea.style.position = 'fixed';
        textArea.style.opacity = '0';
        document.body.appendChild(textArea);
        textArea.select();
        document.execCommand('copy');
        document.body.removeChild(textArea);
        setSuccess('URL copied to clipboard');
      }
      setTimeout(() => setSuccess(null), 2000);
    }
  }, [settings]);

  if (!settings) {
    return (
      <div className="settings-view">
        <div className="settings-loading">Loading settings...</div>
      </div>
    );
  }

  return (
    <div className="settings-view">
      <h2>Settings</h2>

      {error && <div className="settings-error">{error}</div>}
      {success && <div className="settings-success">{success}</div>}

      <section className="settings-section">
        <h3>CC Slots</h3>
        <p className="settings-description">
          MIDI CC mappings for this instance. Rename to match what the CC drives on your synth, or change
          the CC number. Use "↔" to wiggle the CC so your synth's MIDI-learn picks it up.
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
      </section>

      <section className="settings-section">
        <h3>MCP Server</h3>
        <p className="settings-description">
          Connect Claude or other AI tools to control Simply Droplets via MCP.
        </p>

        <div className="settings-row">
          <label>Server URL</label>
          <div className="settings-value-row">
            <code className="settings-code">{settings.mcp_url}</code>
            <button
              className="settings-btn secondary"
              onClick={handleCopyUrl}
              title="Copy URL to clipboard"
            >
              Copy
            </button>
          </div>
        </div>

      </section>

      <section className="settings-section">
        <h3>Custom Instructions</h3>
        <p className="settings-description">
          Appended to the MCP system prompt sent to the AI at session start. Use this to describe your setup —
          synth CC mappings (e.g. "CC20 is wavetable position on Vital"), instance roles
          ("'lead' drives the pad, 'bass' drives the sub"), or stylistic rules ("stay in C minor").
          Leave blank for no extra context. The AI needs to reconnect for changes to take effect.
        </p>

        <textarea
          className="settings-textarea"
          rows={8}
          value={customInstructions}
          onChange={(e) => {
            setCustomInstructions(e.target.value);
            setIsInstructionsDirty(e.target.value !== (settings?.custom_instructions ?? ''));
          }}
          placeholder={"e.g.\n- Instance 'lead' drives Vital, CC20 = wavetable, CC21 = unison\n- Stay in C minor pentatonic\n- Prefer short 2-bar motifs that loop"}
        />

        <div className="settings-actions">
          <button
            className="settings-btn primary"
            onClick={handleSaveInstructions}
            disabled={!isInstructionsDirty || isSavingInstructions}
          >
            {isSavingInstructions ? 'Saving...' : 'Save Instructions'}
          </button>
          <button
            className="settings-btn secondary"
            onClick={handleResetInstructions}
            disabled={!isInstructionsDirty || isSavingInstructions}
          >
            Reset
          </button>
        </div>
      </section>

      <section className="settings-section">
        <h3>MIDI Export</h3>
        <p className="settings-description">
          Exported fugues are saved as Standard MIDI Files (.mid) that you can drag into your DAW.
        </p>

        <div className="settings-row">
          <label>Export Directory</label>
          {isEditing ? (
            <div className="settings-edit-row">
              <input
                type="text"
                className="settings-input"
                value={exportPath}
                onChange={(e) => setExportPath(e.target.value)}
                placeholder="/path/to/exports"
              />
              <button
                className="settings-btn primary"
                onClick={handleSave}
                disabled={isSaving}
              >
                {isSaving ? 'Saving...' : 'Save'}
              </button>
              <button
                className="settings-btn secondary"
                onClick={handleReset}
                disabled={isSaving}
              >
                Cancel
              </button>
            </div>
          ) : (
            <div className="settings-value-row">
              <code className="settings-code path">{settings.export_path}</code>
              <button
                className="settings-btn secondary"
                onClick={() => setIsEditing(true)}
              >
                Edit
              </button>
            </div>
          )}
        </div>

        <div className="settings-actions">
          <button
            className="settings-btn primary"
            onClick={handleRevealExports}
          >
            Open Exports Folder
          </button>
        </div>
      </section>

      <section className="settings-section">
        <h3>About</h3>
        <p className="settings-description">
          Simply Droplets - AI-powered parameter control for your DAW.
        </p>
        <div className="settings-row">
          <label>Version</label>
          <span>0.1.0</span>
        </div>
      </section>
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

  // Keep drafts in sync with poll-refreshed props — but only when the user
  // isn't actively editing. Otherwise their keystrokes would get clobbered
  // on each 1s refresh.
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
      // Revert to current value on invalid input.
      setCcDraft(String(slot.cc ?? ''));
    }
  };

  return (
    <div className={`slot-row ${wiggling ? 'wiggling' : ''}`}>
      <input
        className="slot-name-input"
        value={nameDraft}
        onChange={(e) => setNameDraft(e.target.value)}
        onFocus={() => setNameEditing(true)}
        onBlur={commitName}
        onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur(); }}
      />
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
      <button
        className="wiggle-btn"
        onClick={onWiggle}
        disabled={slot.cc === null || wiggleDisabled}
        title={slot.cc === null ? 'Set a CC first' : 'Wiggle CC to identify knob'}
      >
        {wiggling ? '~' : '↔'}
      </button>
      <button
        className="slot-remove-btn"
        onClick={onRemove}
        title="Remove this slot"
      >
        ×
      </button>
    </div>
  );
};

export default Settings;
