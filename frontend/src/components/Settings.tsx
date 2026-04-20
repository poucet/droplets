/**
 * Settings - Configuration view for export path and MCP server info
 */

import React, { useEffect, useState, useCallback } from 'react';
import {
  getSettings,
  updateSettings,
  revealExports,
  type SettingsResponse,
} from '../api';
import './Settings.css';

export const Settings: React.FC = () => {
  const [settings, setSettings] = useState<SettingsResponse | null>(null);
  const [exportPath, setExportPath] = useState('');
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  // Custom instructions are appended to the MCP system prompt — give the
  // LLM per-setup context like synth CC mappings or stylistic rules.
  const [customInstructions, setCustomInstructions] = useState('');
  const [isInstructionsDirty, setIsInstructionsDirty] = useState(false);
  const [isSavingInstructions, setIsSavingInstructions] = useState(false);

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

    </div>
  );
};

export default Settings;
