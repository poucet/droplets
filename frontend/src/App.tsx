import React, { useEffect, useState, useCallback, useRef } from 'react';
import './App.css';
import {
  getSlots,
  getActivity,
  getFugues,
  getTransport,
  noteOn,
  noteOff,
  wiggleSlot,
  queueFugue,
  cancelFugue,
  RealtimeConnection,
} from './api';
import type {
  SlotInfo,
  FugueInfo,
  FugueDefinition,
  TransportState,
  FuguesResponse,
  ActivityEventDto,
} from './types';
import { FugueList, FugueViewer, FugueComposer } from './components';
import type { ComposerFugue } from './components';

// Note names for display
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const getNoteName = (midi: number) => `${NOTE_NAMES[midi % 12]}${Math.floor(midi / 12) - 1}`;

const DEFAULT_TRANSPORT: TransportState = {
  beat: 0,
  tempo: 120,
  playing: false,
  time_sig_numerator: 4,
};

type TabView = 'sequencer' | 'monitor';

const App: React.FC = () => {
  const [slots, setSlots] = useState<SlotInfo[]>([]);
  const [activity, setActivity] = useState<ActivityEventDto[]>([]);
  const [serverStatus, setServerStatus] = useState<'connecting' | 'connected' | 'error'>('connecting');
  const [wigglingSlot, setWigglingSlot] = useState<number | null>(null);
  const [activeNotes, setActiveNotes] = useState<Set<number>>(new Set());

  // Fugue state
  const [fugueInfos, setFugueInfos] = useState<FugueInfo[]>([]);
  const [fugueDefinitions, setFugueDefinitions] = useState<Map<bigint, FugueDefinition>>(new Map());
  const [transport, setTransport] = useState<TransportState>(DEFAULT_TRANSPORT);
  const [selectedFugueId, setSelectedFugueId] = useState<bigint | undefined>();
  const [showComposer, setShowComposer] = useState(false);

  // Tab navigation
  const [activeTab, setActiveTab] = useState<TabView>('sequencer');

  const realtimeRef = useRef<RealtimeConnection | null>(null);

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
      const defMap = new Map<bigint, FugueDefinition>();
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
      setTransport(response.transport);
    } catch (e) {
      console.error('Failed to fetch transport:', e);
    }
  }, []);

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
  const handleSelectFugue = useCallback((id: bigint) => {
    setSelectedFugueId(id);
  }, []);

  const handleCancelFugue = useCallback(async (id: bigint) => {
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
      // Refresh fugue list
      await fetchFugues();
    } catch (e) {
      console.error('Failed to queue fugue:', e);
    }
  }, [fetchFugues]);

  // Handle realtime updates
  const handleFuguesUpdate = useCallback((response: FuguesResponse) => {
    setFugueInfos(response.infos);
    const defMap = new Map<bigint, FugueDefinition>();
    for (const def of response.definitions) {
      defMap.set(def.id, def);
    }
    setFugueDefinitions(defMap);
  }, []);

  useEffect(() => {
    // Initial fetch
    fetchSlots();
    fetchActivity();
    fetchFugues();
    fetchTransport();

    // Setup realtime connection for transport/fugue updates
    const realtime = new RealtimeConnection({
      onTransport: setTransport,
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
  }, [fetchSlots, fetchActivity, fetchFugues, fetchTransport, handleFuguesUpdate]);

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
        </nav>
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
                onClick={() => setShowComposer(!showComposer)}
              >
                {showComposer ? 'Back to List' : '+ New Fugue'}
              </button>
            </div>

            {showComposer ? (
              <FugueComposer
                onQueue={handleQueueFugue}
                onCancel={() => setShowComposer(false)}
              />
            ) : (
              <div className="sequencer-content">
                <FugueList
                  fugues={fugueInfos}
                  selectedId={selectedFugueId}
                  transport={transport}
                  onSelect={handleSelectFugue}
                  onCancel={handleCancelFugue}
                />

                {selectedFugue && (
                  <FugueViewer
                    fugue={selectedFugue}
                    info={selectedInfo}
                    transport={transport}
                  />
                )}
              </div>
            )}
          </div>
        ) : (
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
                <div className="slots-hint">
                  <p><strong>How to use:</strong></p>
                  <p>1. Map parameters to any plugin via DAW modulation</p>
                  <p>2. AI sets values via MCP <code>set_param</code></p>
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
        )}
      </main>
    </div>
  );
};

export default App;
