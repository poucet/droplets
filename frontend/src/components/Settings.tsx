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
        <h3>Parameter Slots</h3>
        <p className="settings-description">
          16 automatable CC slots for the selected instance. Map these to synth knobs in your
          DAW (right-click → Learn MIDI CC on the target) so the AI can drive them via
          `cc` fugues. Use "wiggle" to jog a slot's CC output for MIDI-learn pickup.
        </p>
        <div className="slots-list">
          {slots.length === 0 ? (
            <div className="no-slots"><p>Loading…</p></div>
          ) : (
            slots.map((slot) => (
              <div key={slot.index} className={`slot-item ${wigglingSlot === slot.index ? 'wiggling' : ''}`}>
                <span className="slot-index">{slot.index}</span>
                <span className="slot-name">{slot.name}</span>
                <span className="slot-cc">{slot.cc !== null ? `CC${slot.cc}` : '—'}</span>
                <div className="slot-bar-container">
                  <div className="slot-bar" style={{ width: `${slot.value * 100}%` }} />
                </div>
                <span className="slot-value">{Math.round(slot.value * 100)}%</span>
                <button
                  className="wiggle-btn"
                  onClick={() => handleWiggle(slot.index)}
                  disabled={slot.cc === null || wigglingSlot !== null}
                  title={slot.cc === null ? 'Map a CC first' : 'Wiggle CC to identify knob'}
                >
                  {wigglingSlot === slot.index ? '~' : '↔'}
                </button>
              </div>
            ))
          )}
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

export default Settings;
