import React, { useEffect, useState, useCallback, useRef } from 'react';
import './App.css';
import {
  getFugues,
  getTransport,
  getInstances,
  getSelf,
  getProjectLayout,
  getSettings,
  renameInstance,
  cancelFugue,
  exportFugue,
  importFugueBytes,
  startDrag,
  RealtimeConnection,
} from './api';
import type {
  FugueInfo,
  FugueDefinition,
  FuguesResponse,
  InstanceInfo,
  ProjectLayout,
  TransportState,
} from './types';

// Stable empty references — returned by the per-instance lookup when the
// selected instance hasn't reported yet. Using module-level constants keeps
// `fugueInfos`/`fugueDefinitions` reference-stable, so memo'd children don't
// thrash on every render.
const EMPTY_INFOS: FugueInfo[] = [];
const EMPTY_DEFS: Map<string, FugueDefinition> = new Map();
import { FugueViewer, MidiMapping, Settings, DawLayout } from './components';
import { useTransport, useTimingSync } from './timing';

type TabView = 'sequencer' | 'midi' | 'settings' | 'daw';

const App: React.FC = () => {
  const [serverStatus, setServerStatus] = useState<'connecting' | 'connected' | 'error'>('connecting');
  // MCP server URL — fetched once at mount, shown in the header as a copy
  // affordance. Doesn't need to update live; the URL is static per-install.
  const [mcpUrl, setMcpUrl] = useState<string>('');
  const [mcpCopied, setMcpCopied] = useState(false);

  // Instance state
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selectedInstance, setSelectedInstance] = useState<string>('default');
  const [selfId, setSelfId] = useState<string | null>(null);
  const [editingInstanceName, setEditingInstanceName] = useState<string | null>(null);

  // Fugue state — per-instance. One WebSocket subscription feeds every
  // instance's data, tagged by instance_id; the UI looks up the selected
  // instance's entry here. Switching the dropdown is then instant (no
  // WS reconnect, no REST round-trip).
  const [fuguesByInstance, setFuguesByInstance] = useState<Map<string, { infos: FugueInfo[]; definitions: Map<string, FugueDefinition> }>>(new Map());

  // Per-instance transport cache — used to re-sync the timing manager on
  // instance switch without a refetch. Effect below runs the re-sync; it
  // lives after `syncTiming` is declared so TS can see the ordering.
  const transportByInstanceRef = useRef<Map<string, TransportState>>(new Map());

  // Ref to the currently-selected instance so the WS closures (set up
  // once on mount) always see the latest selection without remounting.
  const selectedInstanceRef = useRef(selectedInstance);

  // Derived view of the selected instance's fugues.
  const selectedFugues = fuguesByInstance.get(selectedInstance);
  const fugueInfos = selectedFugues?.infos ?? EMPTY_INFOS;
  const fugueDefinitions = selectedFugues?.definitions ?? EMPTY_DEFS;

  // Tab navigation
  const [activeTab, setActiveTab] = useState<TabView>('sequencer');

  // DAW project layout — pushed by the host controller extension over the
  // GUI WebSocket. `null` until the first push; rendered as a "no extension
  // running" empty state.
  const [projectLayout, setProjectLayout] = useState<ProjectLayout | null>(null);
  // DAW tab only makes sense when a host controller extension has actually
  // pushed a layout. Hide it entirely when empty.
  const dawTabVisible = projectLayout !== null && projectLayout.tracks.length > 0;
  const [layoutUpdatedAt, setLayoutUpdatedAt] = useState<number | null>(null);

  // If the DAW layout disappears while the user is on the DAW tab, bounce
  // them back to Sequencer so they don't get stuck on a tab that's no
  // longer in the nav.
  useEffect(() => {
    if (activeTab === 'daw' && !dawTabVisible) {
      setActiveTab('sequencer');
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dawTabVisible]);

  // Client-side interpolated transport (smooth animation)
  const transport = useTransport();
  const syncTiming = useTimingSync();

  // Keep selectedInstanceRef fresh, and re-seed the timing manager from
  // the cached transport when the user switches instances so the playhead
  // doesn't freeze while we wait for the next WS tick.
  useEffect(() => {
    selectedInstanceRef.current = selectedInstance;
    const cached = transportByInstanceRef.current.get(selectedInstance);
    if (cached) {
      syncTiming(cached);
    }
  }, [selectedInstance, syncTiming]);

  const realtimeRef = useRef<RealtimeConnection | null>(null);

  const fetchProjectLayout = useCallback(async () => {
    try {
      const layout = await getProjectLayout();
      if (layout.tracks.length > 0) {
        setProjectLayout(layout);
        setLayoutUpdatedAt(Date.now());
      }
    } catch (e) {
      // Missing endpoint in older backends — silently tolerate so the UI
      // doesn't spam errors before the rebuild lands.
      console.debug('getProjectLayout unavailable:', e);
    }
  }, []);

  const fetchInstances = useCallback(async () => {
    try {
      const response = await getInstances();
      setInstances(response.instances);
      setSelectedInstance(prev => {
        if (prev === 'default' && response.instances.length > 0) {
          return response.instances[0].id;
        }
        return prev;
      });
    } catch (e) {
      console.error('Failed to fetch instances:', e);
    }
  }, []);

  const fetchFugues = useCallback(async () => {
    try {
      const response = await getFugues(selectedInstance);
      const defMap = new Map<string, FugueDefinition>();
      for (const def of response.definitions) {
        defMap.set(def.id, def);
      }
      setFuguesByInstance(prev => {
        const next = new Map(prev);
        next.set(selectedInstance, { infos: response.infos, definitions: defMap });
        return next;
      });
    } catch (e) {
      console.error('Failed to fetch fugues:', e);
    }
  }, [selectedInstance]);

  const fetchTransport = useCallback(async () => {
    try {
      const response = await getTransport(selectedInstance);
      transportByInstanceRef.current.set(selectedInstance, response.transport);
      syncTiming(response.transport);
    } catch (e) {
      console.error('Failed to fetch transport:', e);
    }
  }, [selectedInstance, syncTiming]);

  const handleCancelFugue = useCallback(async (id: string) => {
    try {
      await cancelFugue(id, selectedInstance);
      await fetchFugues();
    } catch (e) {
      console.error('Failed to cancel fugue:', e);
    }
  }, [selectedInstance, fetchFugues]);

  const handleExportFugue = useCallback(async (id: string) => {
    try {
      const response = await exportFugue(id, transport.tempo, selectedInstance);
      if (response.ok) {
        console.log('Exported fugue to:', response.path);
      } else {
        console.error('Failed to export fugue:', response.error);
      }
    } catch (e) {
      console.error('Failed to export fugue:', e);
    }
  }, [selectedInstance, transport.tempo]);

  // Drag-in support: accept .mid files dropped on the sequencer panel and
  // queue them as fugues on the currently-selected instance. `dragDepth`
  // tracks dragenter/leave nesting so child elements don't flicker the
  // highlight off when the cursor crosses internal boundaries.
  const [isDraggingMidi, setIsDraggingMidi] = useState(false);
  const dragDepthRef = useRef(0);
  const hasMidiFile = useCallback((e: React.DragEvent): boolean => {
    // dataTransfer.items carries MIME types during dragenter/over (files
    // themselves aren't exposed until drop). Accept the official audio/midi
    // type plus audio/mid (older) and any item with a .mid/.midi name
    // suffix — browsers disagree on what they report for local drags.
    const items = e.dataTransfer.items;
    if (!items || items.length === 0) return false;
    for (let i = 0; i < items.length; i++) {
      const it = items[i];
      if (it.kind !== 'file') continue;
      if (it.type === 'audio/midi' || it.type === 'audio/mid' || it.type === 'audio/x-midi') {
        return true;
      }
    }
    return true; // fall back to accepting — final filter is on drop.
  }, []);
  const handleDragEnter = useCallback((e: React.DragEvent) => {
    if (!hasMidiFile(e)) return;
    e.preventDefault();
    dragDepthRef.current += 1;
    setIsDraggingMidi(true);
  }, [hasMidiFile]);
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  }, []);
  const handleDragLeave = useCallback(() => {
    dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
    if (dragDepthRef.current === 0) setIsDraggingMidi(false);
  }, []);
  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    dragDepthRef.current = 0;
    setIsDraggingMidi(false);
    const files = Array.from(e.dataTransfer.files ?? []).filter(
      f => f.name.toLowerCase().endsWith('.mid') || f.name.toLowerCase().endsWith('.midi')
    );
    if (files.length === 0) return;
    // Import sequentially so log output is ordered and a single failure
    // doesn't swallow other files' errors. One fetch per file; the backend
    // handles multi-track files by producing one fugue per track.
    for (const file of files) {
      try {
        const bytes = await file.arrayBuffer();
        const result = await importFugueBytes(bytes, selectedInstance);
        if (!result.ok) {
          console.error('[droplets] import_fugue failed:', result.error);
        } else {
          console.debug(
            '[droplets] imported', file.name, '→', result.fugue_ids?.length ?? 0, 'fugue(s)'
          );
        }
      } catch (err) {
        console.error('[droplets] import error for', file.name, err);
      }
    }
    // One refresh covers all drops — the WS push should catch up anyway,
    // but a deterministic fetch makes the UI update feel instant.
    fetchFugues();
  }, [selectedInstance, fetchFugues]);

  // Handle realtime updates
  const handleFuguesUpdate = useCallback((instanceId: string, response: FuguesResponse) => {
    const defMap = new Map<string, FugueDefinition>();
    for (const def of response.definitions) {
      defMap.set(def.id, def);
    }
    setFuguesByInstance(prev => {
      const next = new Map(prev);
      next.set(instanceId, { infos: response.infos, definitions: defMap });
      return next;
    });
  }, []);

  // Handle instance rename
  const handleRenameInstance = useCallback(async (newName: string) => {
    const trimmed = newName.trim();
    if (!trimmed) {
      setEditingInstanceName(null);
      return;
    }
    try {
      await renameInstance(selectedInstance, trimmed);
      await fetchInstances();
    } catch (e) {
      console.error('Failed to rename instance:', e);
    }
    setEditingInstanceName(null);
  }, [selectedInstance, fetchInstances]);

  // Mount-once effect: WS + one-shot initial fetches.
  useEffect(() => {
    getSelf().then(r => {
      setSelfId(r.id);
      setSelectedInstance(r.id);
    }).catch(() => {});
    getSettings().then(s => setMcpUrl(s.mcp_url)).catch(() => {});
    fetchInstances();
    fetchProjectLayout();

    const realtime = new RealtimeConnection({
      onTransport: (instanceId, transport) => {
        // Cache every instance's transport. Only drive the timing manager
        // when the currently-selected instance reports, so playhead
        // animation doesn't flicker between DAWs.
        transportByInstanceRef.current.set(instanceId, transport);
        if (instanceId === selectedInstanceRef.current) {
          syncTiming(transport);
        }
      },
      onFugues: handleFuguesUpdate,
      onProjectLayout: (layout) => {
        setProjectLayout(layout);
        setLayoutUpdatedAt(Date.now());
      },
      onConnect: () => setServerStatus('connected'),
      onDisconnect: () => setServerStatus('connecting'),
      onError: () => setServerStatus('error'),
    });
    realtime.connect();
    realtimeRef.current = realtime;

    return () => {
      realtime.disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // On instance change: refill caches for the new selection so the UI
  // shows data immediately (the WS will keep them fresh afterward).
  useEffect(() => {
    fetchFugues();
    fetchTransport();
  }, [selectedInstance, fetchFugues, fetchTransport]);

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1>
            Simply Droplets
            <span className="app-version">0.1.0</span>
          </h1>
          <span className="subtitle">AI Parameter Bridge</span>
        </div>
        <nav className="tab-nav">
          <button
            className={`tab-btn ${activeTab === 'sequencer' ? 'active' : ''}`}
            onClick={() => setActiveTab('sequencer')}
          >
            Sequencer
          </button>
          <button
            className={`tab-btn ${activeTab === 'midi' ? 'active' : ''}`}
            onClick={() => setActiveTab('midi')}
          >
            MIDI Mapping
          </button>
          <button
            className={`tab-btn ${activeTab === 'settings' ? 'active' : ''}`}
            onClick={() => setActiveTab('settings')}
          >
            Settings
          </button>
          {dawTabVisible && (
            <button
              className={`tab-btn ${activeTab === 'daw' ? 'active' : ''}`}
              onClick={() => setActiveTab('daw')}
            >
              DAW
            </button>
          )}
        </nav>
        <div className="instance-selector">
          {instances.length > 1 && (
            <select
              value={selectedInstance}
              onChange={(e) => setSelectedInstance(e.target.value)}
              className={`instance-dropdown${selectedInstance === selfId ? ' instance-dropdown--self' : ''}`}
            >
              {instances.map((inst) => (
                <option key={inst.id} value={inst.id}>
                  {inst.id === selfId ? '● ' : '○ '}{inst.name}
                </option>
              ))}
            </select>
          )}
          {editingInstanceName !== null ? (
            <input
              type="text"
              className="instance-name-input"
              value={editingInstanceName}
              onChange={(e) => setEditingInstanceName(e.target.value)}
              onBlur={() => handleRenameInstance(editingInstanceName)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleRenameInstance(editingInstanceName);
                if (e.key === 'Escape') setEditingInstanceName(null);
              }}
              autoFocus
            />
          ) : (
            <span
              className="instance-name"
              onClick={() => {
                const inst = instances.find(i => i.id === selectedInstance);
                setEditingInstanceName(inst?.name ?? selectedInstance);
              }}
              title="Click to rename instance"
            >
              {instances.find(i => i.id === selectedInstance)?.name ?? selectedInstance}
            </span>
          )}
        </div>
        <div className="header-right">
          <div className="header-right-top">
            <div className="transport-info">
              <span className="transport-beat">{transport.beat.toFixed(2)}</span>
              <span className="transport-tempo">{transport.tempo.toFixed(0)} BPM</span>
              <span className={`transport-status ${transport.playing ? 'playing' : 'stopped'}`}>
                {transport.playing ? '▶' : '■'}
              </span>
            </div>
            <div className="server-status">
              <span className={`status-dot ${serverStatus}`}></span>
            </div>
          </div>
          {mcpUrl && (
            <button
              className={`mcp-url ${mcpCopied ? 'copied' : ''}`}
              title="Click to copy MCP server URL"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(mcpUrl);
                  setMcpCopied(true);
                  setTimeout(() => setMcpCopied(false), 1500);
                } catch {
                  // Clipboard API can fail in non-secure contexts; best-effort
                }
              }}
            >
              <span className="mcp-url-label">MCP</span>
              <code className="mcp-url-value">{mcpUrl}</code>
              {mcpCopied && <span className="mcp-url-status">copied</span>}
            </button>
          )}
        </div>
      </header>

      <main className="app-main">
        {activeTab === 'sequencer' ? (
          /* Sequencer Tab — read-only view of what's playing. Composing is
             LLM-driven via MCP; the UI used to have an in-app editor, but it
             duplicated the DAW's piano roll badly and was removed. */
          <div
            className={`sequencer-view${isDraggingMidi ? ' is-drop-target' : ''}`}
            onDragEnter={handleDragEnter}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
          >
            <div className="sequencer-header">
              <h2>Fugue Sequencer</h2>
              {/* Drop hint only renders while a drag is in flight — avoids
                  UI noise for users who never use the drag-in feature. */}
              {isDraggingMidi && (
                <span className="drop-hint">
                  Drop <code>.mid</code> to import into{' '}
                  <strong>{instances.find(i => i.id === selectedInstance)?.name ?? selectedInstance}</strong>
                </span>
              )}
            </div>
            <div className="sequencer-content">
              {fugueInfos.length === 0 && (
                <div className="fugue-empty">
                  <p className="empty-message">No active fugues</p>
                  <p className="empty-hint">Queue a fugue to see it here</p>
                </div>
              )}
              {fugueInfos.length > 1 && (
                <div className="fugue-toolbar">
                  <button
                    className="drag-all-btn"
                    onMouseDown={(e) => {
                      e.stopPropagation();
                      e.preventDefault();
                      startDrag({ instance: selectedInstance, active: true, tempo_bpm: transport.tempo });
                    }}
                    title={`Drag all ${fugueInfos.length} fugues as one .mid into your DAW`}
                  >
                    ⇣ Drag all <span className="drag-all-count">{fugueInfos.length}</span>
                  </button>
                </div>
              )}
              {fugueInfos.map(info => {
                const def = fugueDefinitions.get(info.id);
                return def ? (
                  <FugueViewer
                    key={info.id}
                    fugue={def}
                    info={info}
                    instance={selectedInstance}
                    tempoBpm={transport.tempo}
                    onCancel={handleCancelFugue}
                    onExport={handleExportFugue}
                  />
                ) : null;
              })}
            </div>
          </div>
        ) : activeTab === 'midi' ? (
          /* MIDI Mapping Tab — per-instance CC slot editor */
          <MidiMapping instance={selectedInstance} />
        ) : activeTab === 'daw' ? (
          /* DAW Tab — live view of what the host controller extension has pushed */
          <DawLayout
            layout={projectLayout}
            lastUpdatedAt={layoutUpdatedAt}
            hostConnected={projectLayout !== null && projectLayout.tracks.length > 0}
          />
        ) : activeTab === 'settings' ? (
          /* Settings Tab */
          <Settings />
        ) : null}
      </main>
    </div>
  );
};

export default App;
