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

  const fetchSettings = useCallback(async () => {
    try {
      const response = await getSettings();
      setSettings(response);
      setExportPath(response.export_path);
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

  const handleRevealExports = useCallback(async () => {
    try {
      await revealExports();
    } catch (e) {
      console.error('Failed to open exports folder:', e);
      setError('Failed to open exports folder');
    }
  }, []);

  const handleCopyUrl = useCallback(() => {
    if (settings) {
      navigator.clipboard.writeText(settings.mcp_url);
      setSuccess('URL copied to clipboard');
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

        <div className="settings-row">
          <label>Port</label>
          <code className="settings-code">{settings.mcp_port}</code>
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
