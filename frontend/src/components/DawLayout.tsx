import React, { useEffect, useRef } from 'react';
import './DawLayout.css';
import type { ProjectLayout, TrackContext, Device, DrumPad } from '../types';

// MIDI number → pitch notation. Mirrors the backend `midi_to_name`
// helper so what we render on the UI matches the MCP-visible view.
// DAW convention: C3 = middle C = MIDI 60 (Bitwig/Ableton/Logic/Reaper).
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const midiToName = (midi: number) => `${NOTE_NAMES[midi % 12]}${Math.floor(midi / 12) - 2}`;

interface DawLayoutProps {
  layout: ProjectLayout | null;
  lastUpdatedAt: number | null;
  hostConnected: boolean;
}

/**
 * DAW tab — visualizes the last-known project layout pushed by the host
 * controller extension (Bitwig / Ableton / ...). The goal is end-to-end
 * verification: you should be able to rename a track or swap a sample in
 * the DAW and see it reflected here without polling.
 *
 * When no layout is available (`hostConnected=false`), the UI explains what
 * the tab WOULD show and how to bring it online — matches the graceful
 * degradation the MCP tools already do for the LLM.
 */
const DawLayout: React.FC<DawLayoutProps> = ({ layout, lastUpdatedAt, hostConnected }) => {
  const [showRawJson, setShowRawJson] = React.useState(false);

  // Only show tracks that actually have devices. Natively filters out
  // Bitwig's master/FX/return tracks and any empty audio/MIDI tracks —
  // the user wants to see what's on the project, not an inventory.
  const visibleTracks = React.useMemo(
    () => (layout?.tracks ?? []).filter((t) => t.devices.length > 0),
    [layout],
  );

  // Raw JSON view is always available for debugging, even when no layout
  // has arrived — seeing `null` vs an empty `{tracks:[]}` tells the user
  // whether the frontend just hasn't received anything, or received an
  // empty push.
  const rawToggle = (
    <button
      className="daw-raw-toggle"
      onClick={() => setShowRawJson((v) => !v)}
      title="Toggle raw JSON payload (for end-to-end debugging)"
    >
      {showRawJson ? 'Hide raw' : 'Show raw'}
    </button>
  );

  if (!hostConnected || !layout || visibleTracks.length === 0) {
    return (
      <div className="daw-layout daw-layout--empty">
        <div className="daw-empty-card">
          <div className="daw-status-dot daw-status-dot--off" />
          <h2>No DAW layout received</h2>
          <p>
            Load the <strong>Droplets host extension</strong> in your DAW (Bitwig:
            Settings → Controllers → add "Simply Droplets") and it will push the
            project's track and device tree here.
          </p>
          <p className="daw-empty-hint">
            Without this the AI can still compose, but it doesn't know what
            sample is on each drum pad or what synth is on each track.
          </p>
          <div className="daw-empty-debug">
            {rawToggle}
            {showRawJson && (
              <pre className="daw-raw-json">{JSON.stringify(layout, null, 2)}</pre>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="daw-layout">
      <header className="daw-header">
        <div className="daw-header-left">
          <div className="daw-status-dot daw-status-dot--on" />
          <span className="daw-header-label">DAW layout live</span>
        </div>
        <div className="daw-header-right">
          {lastUpdatedAt !== null && (
            <LiveUpdateTimer lastUpdatedAt={lastUpdatedAt} />
          )}
          <span className="daw-track-count">
            {visibleTracks.length} track{visibleTracks.length === 1 ? '' : 's'}
            {layout.tracks.length !== visibleTracks.length && (
              <span className="daw-track-count-hint">
                {' '}({layout.tracks.length - visibleTracks.length} empty hidden)
              </span>
            )}
          </span>
          {rawToggle}
        </div>
      </header>

      {showRawJson && (
        <pre className="daw-raw-json">{JSON.stringify(layout, null, 2)}</pre>
      )}

      <div className="daw-track-list">
        {visibleTracks.map((track, idx) => (
          <TrackCard key={`${track.track_name}-${idx}`} track={track} />
        ))}
      </div>
    </div>
  );
};

/** Shows "updated 2s ago" with a ticking update to convey liveness. */
const LiveUpdateTimer: React.FC<{ lastUpdatedAt: number }> = ({ lastUpdatedAt }) => {
  const [, force] = React.useState(0);
  useEffect(() => {
    const t = setInterval(() => force(x => x + 1), 1000);
    return () => clearInterval(t);
  }, []);
  const elapsed = Math.max(0, Math.floor((Date.now() - lastUpdatedAt) / 1000));
  const label = elapsed < 2 ? 'just now' : `${elapsed}s ago`;
  return <span className="daw-last-update">updated {label}</span>;
};

/**
 * Single track card. Highlights briefly when its identity-defining fields
 * change so the user visually sees pushes arriving.
 */
const TrackCard: React.FC<{ track: TrackContext }> = ({ track }) => {
  const signature = JSON.stringify(track);
  const prevSignature = useRef(signature);
  const [flash, setFlash] = React.useState(false);

  useEffect(() => {
    if (prevSignature.current !== signature) {
      prevSignature.current = signature;
      setFlash(true);
      const t = setTimeout(() => setFlash(false), 600);
      return () => clearTimeout(t);
    }
  }, [signature]);

  const hasDroplets = track.droplets_instance_id !== null && track.droplets_instance_id !== undefined;

  return (
    <div className={`daw-track${flash ? ' daw-track--flash' : ''}${hasDroplets ? ' daw-track--droplets' : ''}`}>
      <div className="daw-track-header">
        <h3 className="daw-track-name">{track.track_name}</h3>
        {hasDroplets && (
          <span
            className="daw-droplets-badge"
            title={track.droplets_instance_id ?? undefined}
          >
            Droplets
          </span>
        )}
      </div>
      {track.devices.length === 0 ? (
        <div className="daw-track-empty">No devices</div>
      ) : (
        <div className="daw-device-chain">
          {track.devices.map((device, idx) => (
            <DeviceCell key={idx} device={device} />
          ))}
        </div>
      )}
    </div>
  );
};

/** One device in a chain. Drum machines expand into a pad grid inline. */
const DeviceCell: React.FC<{ device: Device }> = ({ device }) => {
  switch (device.type) {
    case 'instrument':
      return (
        <div className="daw-device daw-device--instrument">
          <div className="daw-device-type">Instrument</div>
          <div className="daw-device-name">{device.name}</div>
          {device.preset_name && <div className="daw-device-preset">{device.preset_name}</div>}
          {device.sample_name && <div className="daw-device-sample">♪ {device.sample_name}</div>}
          {device.vendor && <div className="daw-device-vendor">{device.vendor}</div>}
        </div>
      );
    case 'effect':
      return (
        <div className="daw-device daw-device--effect">
          <div className="daw-device-type">Effect</div>
          <div className="daw-device-name">{device.name}</div>
          {device.preset_name && <div className="daw-device-preset">{device.preset_name}</div>}
          {device.vendor && <div className="daw-device-vendor">{device.vendor}</div>}
        </div>
      );
    case 'drum_machine':
      return <DrumMachineCell name={device.name} pads={device.pads} />;
    case 'container':
      return (
        <div className="daw-device daw-device--container">
          <div className="daw-device-type">{device.kind}</div>
          <div className="daw-device-name">{device.name}</div>
          <div className="daw-device-vendor">
            {device.chains.length} chain{device.chains.length === 1 ? '' : 's'}
          </div>
        </div>
      );
    case 'unknown':
      return (
        <div className="daw-device daw-device--unknown">
          <div className="daw-device-type">?</div>
          <div className="daw-device-name">{device.name}</div>
          {device.vendor && <div className="daw-device-vendor">{device.vendor}</div>}
        </div>
      );
  }
};

/** Inline pad grid for a Drum Machine. Shows note, pad name, sample. */
const DrumMachineCell: React.FC<{ name: string; pads: DrumPad[] }> = ({ name, pads }) => {
  return (
    <div className="daw-device daw-device--drum-machine">
      <div className="daw-device-type">Drum Machine</div>
      <div className="daw-device-name">{name}</div>
      {pads.length === 0 ? (
        <div className="daw-drum-empty">No pads loaded</div>
      ) : (
        <div className="daw-pad-grid">
          {pads.map((pad) => (
            <PadCell key={pad.note} pad={pad} />
          ))}
        </div>
      )}
    </div>
  );
};

const PadCell: React.FC<{ pad: DrumPad }> = ({ pad }) => {
  // Prefer a sample_name from a nested Sampler, fall back to preset, then
  // to the pad's own name. Mirrors what `get_project_state` surfaces at
  // tier 1 so the UI matches what the LLM sees.
  const sampleFromSampler = pad.devices.find(
    (d): d is Extract<Device, { type: 'instrument' }> =>
      d.type === 'instrument' && !!d.sample_name,
  );
  const display = sampleFromSampler?.sample_name ?? pad.devices[0]?.name ?? pad.name;
  return (
    <div className="daw-pad" title={`${midiToName(pad.note)} — ${pad.name}`}>
      <div className="daw-pad-note">{midiToName(pad.note)}</div>
      <div className="daw-pad-name">{pad.name}</div>
      <div className="daw-pad-sample">{display}</div>
    </div>
  );
};

export default DawLayout;
