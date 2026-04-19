import React, { useEffect, useState, useCallback, useRef } from 'react';
import './App.css';
import {
  getSlots,
  getActivity,
  getFugues,
  getTransport,
  getInstances,
  getSelf,
  renameInstance,
  noteOn,
  noteOff,
  wiggleSlot,
  queueFugue,
  cancelFugue,
  exportFugue,
  RealtimeConnection,
} from './api';
import type {
  SlotInfo,
  FugueInfo,
  FugueDefinition,
  FuguesResponse,
  ActivityEventDto,
  InstanceInfo,
} from './types';
import { FugueList, FugueViewer, FugueComposer, Settings } from './components';
import type { ComposerFugue } from './components';
import { useTransport, useTimingSync } from './timing';

// Note names for display
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const getNoteName = (midi: number) => `${NOTE_NAMES[midi % 12]}${Math.floor(midi / 12) - 1}`;

type TabView = 'sequencer' | 'monitor' | 'settings';

const App: React.FC = () => {
  const [slots, setSlots] = useState<SlotInfo[]>([]);
  const [activity, setActivity] = useState<ActivityEventDto[]>([]);
  const [serverStatus, setServerStatus] = useState<'connecting' | 'connected' | 'error'>('connecting');
  const [wigglingSlot, setWigglingSlot] = useState<number | null>(null);
  const [activeNotes, setActiveNotes] = useState<Set<number>>(new Set());

  // Instance state
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selectedInstance, setSelectedInstance] = useState<string>('default');
  const [selfId, setSelfId] = useState<string | null>(null);
  const [editingInstanceName, setEditingInstanceName] = useState<string | null>(null);

  // Fugue state
  const [fugueInfos, setFugueInfos] = useState<FugueInfo[]>([]);
  const [fugueDefinitions, setFugueDefinitions] = useState<Map<string, FugueDefinition>>(new Map());
  const [selectedFugueId, setSelectedFugueId] = useState<string | undefined>();
  const [showComposer, setShowComposer] = useState(false);
  const [editingFugue, setEditingFugue] = useState<FugueDefinition | null>(null);

  // Tab navigation
  const [activeTab, setActiveTab] = useState<TabView>('sequencer');

  // Client-side interpolated transport (smooth animation)
  const transport = useTransport();
  const syncTiming = useTimingSync();

  const realtimeRef = useRef<RealtimeConnection | null>(null);

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

  const fetchSlots = useCallback(async () => {
    try {
      const response = await getSlots();
      setSlots(response.slots);
      setServerStatus('connected');
    } catch (e) {
      console.error('Failed to fetch slots:', e);
      setServerStatus('error');
    }
  }, []);

  const fetchActivity = useCallback(async () => {
    try {
      const response = await getActivity();
      setActivity(response.events);
    } catch (e) {
      console.error('Failed to fetch activity:', e);
    }
  }, []);

  const fetchFugues = useCallback(async () => {
    try {
      const response = await getFugues();
      setFugueInfos(response.infos);
      const defMap = new Map<string, FugueDefinition>();
      for (const def of response.definitions) {
        defMap.set(def.id, def);
      }
      setFugueDefinitions(defMap);
    } catch (e) {
      console.error('Failed to fetch fugues:', e);
    }
  }, []);

  const fetchTransport = useCallback(async () => {
    try {
      const response = await getTransport();
      syncTiming(response.transport);
    } catch (e) {
      console.error('Failed to fetch transport:', e);
    }
  }, [syncTiming]);

  const handleWiggle = useCallback(async (slotIndex: number) => {
    if (wigglingSlot !== null) return;
    setWigglingSlot(slotIndex);
    try {
      await wiggleSlot(slotIndex);
    } catch (e) {
      console.error('Failed to wiggle:', e);
    }
    setTimeout(() => setWigglingSlot(null), 1100);
  }, [wigglingSlot]);

  const handleNoteOn = useCallback(async (note: number) => {
    setActiveNotes(prev => new Set(prev).add(note));
    try {
      await noteOn(note, 100);
    } catch (e) {
      console.error('Failed to send note on:', e);
    }
  }, []);

  const handleNoteOff = useCallback(async (note: number) => {
    setActiveNotes(prev => {
      const next = new Set(prev);
      next.delete(note);
      return next;
    });
    try {
      await noteOff(note);
    } catch (e) {
      console.error('Failed to send note off:', e);
    }
  }, []);

  // Fugue handlers
  const handleSelectFugue = useCallback((id: string) => {
    setSelectedFugueId(id);
  }, []);

  const handleCancelFugue = useCallback(async (id: string) => {
    try {
      await cancelFugue(id);
      // Clear selection if we cancelled the selected fugue
      if (selectedFugueId === id) {
        setSelectedFugueId(undefined);
      }
      // Refresh fugue list
      await fetchFugues();
    } catch (e) {
      console.error('Failed to cancel fugue:', e);
    }
  }, [selectedFugueId, fetchFugues]);

  const handleQueueFugue = useCallback(async (fugue: ComposerFugue) => {
    try {
      await queueFugue(fugue);
      setShowComposer(false);
      setEditingFugue(null);
      // Refresh fugue list
      await fetchFugues();
    } catch (e) {
      console.error('Failed to queue fugue:', e);
    }
  }, [fetchFugues]);

  const handleEditFugue = useCallback((fugue: FugueDefinition) => {
    setEditingFugue(fugue);
    setShowComposer(true);
  }, []);

  const handleExportFugue = useCallback(async (id: string) => {
    try {
      const response = await exportFugue(id, transport.tempo);
      if (response.ok) {
        console.log('Exported fugue to:', response.path);
      } else {
        console.error('Failed to export fugue:', response.error);
      }
    } catch (e) {
      console.error('Failed to export fugue:', e);
    }
  }, [transport.tempo]);

  // Handle realtime updates
  const handleFuguesUpdate = useCallback((response: FuguesResponse) => {
    setFugueInfos(response.infos);
    const defMap = new Map<string, FugueDefinition>();
    for (const def of response.definitions) {
      defMap.set(def.id, def);
    }
    setFugueDefinitions(defMap);
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

  useEffect(() => {
    getSelf().then(r => {
      setSelfId(r.id);
      setSelectedInstance(r.id);
    }).catch(() => {});
    // Initial fetch
    fetchInstances();
    fetchSlots();
    fetchActivity();
    fetchFugues();
    fetchTransport();

    // Setup realtime connection for transport/fugue updates
    const realtime = new RealtimeConnection({
      onTransport: syncTiming,
      onFugues: handleFuguesUpdate,
      onConnect: () => setServerStatus('connected'),
      onDisconnect: () => setServerStatus('connecting'),
      onError: () => setServerStatus('error'),
    });
    realtime.connect();
    realtimeRef.current = realtime;

    // Fallback polling for slots/activity (these don't have realtime yet)
    const interval = setInterval(() => {
      fetchSlots();
      fetchActivity();
    }, 100);

    return () => {
      clearInterval(interval);
      realtime.disconnect();
    };
  }, [fetchInstances, fetchSlots, fetchActivity, fetchFugues, fetchTransport, handleFuguesUpdate, syncTiming]);

  const formatTimestamp = (ts: bigint) => {
    const date = new Date(Number(ts));
    return date.toLocaleTimeString('en-US', {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    });
  };

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
            className={`tab-btn ${activeTab === 'monitor' ? 'active' : ''}`}
            onClick={() => setActiveTab('monitor')}
          >
            Monitor
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
          /* Sequencer Tab */
          <div className="sequencer-view">
            <div className="sequencer-header">
              <h2>Fugue Sequencer</h2>
              <button
                className="new-fugue-btn"
                onClick={() => {
                  if (showComposer) {
                    setShowComposer(false);
                    setEditingFugue(null);
                  } else {
                    setShowComposer(true);
                    setEditingFugue(null);
                  }
                }}
              >
                {showComposer ? 'Back to List' : '+ New Fugue'}
              </button>
            </div>

            {showComposer ? (
              <FugueComposer
                onQueue={handleQueueFugue}
                onCancel={() => {
                  setShowComposer(false);
                  setEditingFugue(null);
                }}
                initialFugue={editingFugue ?? undefined}
              />
            ) : (
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
                    onEdit={handleEditFugue}
                  />
                )}
              </div>
            )}
          </div>
        ) : activeTab === 'monitor' ? (
          /* Monitor Tab */
          <div className="monitor-view">
            <div className="monitor-columns">
              <section className="slots-section">
                <h2>Automatable Parameters</h2>
                <div className="slots-list">
                  {slots.length === 0 ? (
                    <div className="no-slots">
                      <p>Loading parameter slots...</p>
                    </div>
                  ) : (
                    slots.map((slot) => (
                      <div key={slot.index} className={`slot-item ${wigglingSlot === slot.index ? 'wiggling' : ''}`}>
                        <span className="slot-index">{slot.index}</span>
                        <span className="slot-name">{slot.name}</span>
                        <span className="slot-cc">{slot.cc !== null ? `CC${slot.cc}` : '—'}</span>
                        <div className="slot-bar-container">
                          <div
                            className="slot-bar"
                            style={{ width: `${slot.value * 100}%` }}
                          />
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

              <section className="activity-section">
                <h2>Recent AI Activity</h2>
                <div className="activity-list">
                  {activity.length === 0 ? (
                    <div className="no-activity">
                      <p>No recent activity</p>
                      <p className="hint">Activity appears when AI sets parameter values</p>
                    </div>
                  ) : (
                    activity.slice(-20).reverse().map((event, idx) => (
                      <div key={`${event.timestamp}-${idx}`} className="activity-item">
                        <span className="activity-time">{formatTimestamp(event.timestamp)}</span>
                        <span className="activity-instance">{event.instance}</span>
                        <span className="activity-cc">{event.cc !== null ? `CC${event.cc}` : '—'}</span>
                        <span className="activity-value">{event.value}</span>
                      </div>
                    ))
                  )}
                </div>
              </section>
            </div>

            <section className="note-grid-section">
              <h2>Note Test Grid</h2>
              <div className="note-grid">
                {Array.from({ length: 24 }, (_, i) => 48 + i).map(note => {
                  const isBlack = [1, 3, 6, 8, 10].includes(note % 12);
                  const isActive = activeNotes.has(note);
                  return (
                    <button
                      key={note}
                      className={`note-key ${isBlack ? 'black' : 'white'} ${isActive ? 'active' : ''}`}
                      onMouseDown={() => handleNoteOn(note)}
                      onMouseUp={() => handleNoteOff(note)}
                      onMouseLeave={() => isActive && handleNoteOff(note)}
                      title={getNoteName(note)}
                    >
                      {!isBlack && <span className="note-label">{getNoteName(note)}</span>}
                    </button>
                  );
                })}
              </div>
              <p className="note-hint">Click and hold to play notes. Tests MIDI output routing.</p>
            </section>
          </div>
        ) : activeTab === 'settings' ? (
          /* Settings Tab */
          <Settings />
        ) : null}
      </main>
    </div>
  );
};

export default App;
