import React, { useEffect, useState, useCallback, useRef } from 'react';
import './App.css';
import {
  getFugues,
  getTransport,
  getInstances,
  getSelf,
  getProjectLayout,
  renameInstance,
  cancelFugue,
  exportFugue,
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
import { FugueList, FugueViewer, Settings, DawLayout } from './components';
import { useTransport, useTimingSync } from './timing';

type TabView = 'sequencer' | 'daw' | 'settings';

const App: React.FC = () => {
  const [serverStatus, setServerStatus] = useState<'connecting' | 'connected' | 'error'>('connecting');

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
  const [selectedFugueId, setSelectedFugueId] = useState<string | undefined>();

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
  const [layoutUpdatedAt, setLayoutUpdatedAt] = useState<number | null>(null);

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

  // Fugue handlers
  const handleSelectFugue = useCallback((id: string) => {
    setSelectedFugueId(id);
  }, []);

  const handleCancelFugue = useCallback(async (id: string) => {
    try {
      await cancelFugue(id, selectedInstance);
      // Clear selection if we cancelled the selected fugue
      if (selectedFugueId === id) {
        setSelectedFugueId(undefined);
      }
      // Refresh fugue list
      await fetchFugues();
    } catch (e) {
      console.error('Failed to cancel fugue:', e);
    }
  }, [selectedInstance, selectedFugueId, fetchFugues]);

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

  // Get selected fugue definition
  const selectedFugue = selectedFugueId ? fugueDefinitions.get(selectedFugueId) : undefined;
  const selectedInfo = selectedFugueId ? fugueInfos.find(f => f.id === selectedFugueId) : undefined;

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1>Simply Droplets</h1>
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
            className={`tab-btn ${activeTab === 'daw' ? 'active' : ''}`}
            onClick={() => setActiveTab('daw')}
          >
            DAW
          </button>
          <button
            className={`tab-btn ${activeTab === 'settings' ? 'active' : ''}`}
            onClick={() => setActiveTab('settings')}
          >
            Settings
          </button>
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
      </header>

      <main className="app-main">
        {activeTab === 'sequencer' ? (
          /* Sequencer Tab — read-only view of what's playing. Composing is
             LLM-driven via MCP; the UI used to have an in-app editor, but it
             duplicated the DAW's piano roll badly and was removed. */
          <div className="sequencer-view">
            <div className="sequencer-header">
              <h2>Fugue Sequencer</h2>
            </div>
            <div className="sequencer-content">
              <FugueList
                fugues={fugueInfos}
                selectedId={selectedFugueId}
                onSelect={handleSelectFugue}
                onCancel={handleCancelFugue}
                onExport={handleExportFugue}
              />

              {selectedFugue && (
                <FugueViewer
                  fugue={selectedFugue}
                  info={selectedInfo}
                />
              )}
            </div>
          </div>
        ) : activeTab === 'daw' ? (
          /* DAW Tab — live view of what the host controller extension has pushed */
          <DawLayout
            layout={projectLayout}
            lastUpdatedAt={layoutUpdatedAt}
            hostConnected={projectLayout !== null && projectLayout.tracks.length > 0}
          />
        ) : activeTab === 'settings' ? (
          /* Settings Tab */
          <Settings instance={selectedInstance} />
        ) : null}
      </main>
    </div>
  );
};

export default App;
