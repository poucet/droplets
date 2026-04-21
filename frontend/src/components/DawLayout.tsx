import React, { useEffect, useRef } from 'react';
import './DawLayout.css';
import type { ProjectLayout, TrackContext, PrimaryDevice, PadSummary } from '../types';

interface DawLayoutProps {
  layout: ProjectLayout | null;
  lastUpdatedAt: number | null;
  hostConnected: boolean;
}

/**
 * DAW tab — visualizes the last-known project layout pushed by the host
 * controller extension (Bitwig / Ableton / ...). End-to-end verification:
 * rename a track or swap a sample in the DAW, see it reflected here without
 * polling.
 *
 * The wire format carries a single `primary_device` per track (instrument or
 * drum machine) rather than a full device chain — matches what
 * `get_project_state` surfaces to the LLM.
 */
const DawLayout: React.FC<DawLayoutProps> = ({ layout, lastUpdatedAt, hostConnected }) => {
  const [showRawJson, setShowRawJson] = React.useState(false);

  // Only show tracks with a primary sound source — filters out Bitwig's
  // master/FX/return tracks and empty audio/MIDI tracks.
  const visibleTracks = React.useMemo(
    () => (layout?.tracks ?? []).filter((t) => t.primary_device != null),
    [layout],
  );

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
            Settings → Controllers → add "Droplets") and it will push the
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
      {track.primary_device ? (
        <div className="daw-device-chain">
          <PrimaryDeviceCell device={track.primary_device} />
        </div>
      ) : (
        <div className="daw-track-empty">No primary device</div>
      )}
    </div>
  );
};

/** Render the track's primary device — an instrument or a drum machine. */
const PrimaryDeviceCell: React.FC<{ device: PrimaryDevice }> = ({ device }) => {
  switch (device.type) {
    case 'instrument':
      return (
        <div className="daw-device daw-device--instrument">
          <div className="daw-device-type">Instrument</div>
          <div className="daw-device-name">{device.name}</div>
          {device.preset_name && <div className="daw-device-preset">{device.preset_name}</div>}
          {device.vendor && <div className="daw-device-vendor">{device.vendor}</div>}
        </div>
      );
    case 'drum_machine':
      return <DrumMachineCell name={device.name} pads={device.pads} />;
  }
};

/** Inline pad grid for a Drum Machine. Shows note, pad name, sample. */
const DrumMachineCell: React.FC<{ name: string; pads: PadSummary[] }> = ({ name, pads }) => {
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

const PadCell: React.FC<{ pad: PadSummary }> = ({ pad }) => {
  const display = pad.sample_name ?? pad.name;
  return (
    <div className="daw-pad" title={`${pad.note} — ${pad.name}`}>
      <div className="daw-pad-note">{pad.note}</div>
      <div className="daw-pad-name">{pad.name}</div>
      <div className="daw-pad-sample">{display}</div>
    </div>
  );
};

export default DawLayout;
