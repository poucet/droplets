import React, { useEffect, useState, useCallback } from 'react';
import './App.css';

interface SlotInfo {
  index: number;
  name: string;
  cc: number | null;
  channel: number;
  value: number;
  learning: boolean;
}

interface ActivityEvent {
  timestamp: number;
  instance: string;
  channel: number;
  cc: number;
  value: number;
}

const App: React.FC = () => {
  const [slots, setSlots] = useState<SlotInfo[]>([]);
  const [activity, setActivity] = useState<ActivityEvent[]>([]);
  const [serverStatus, setServerStatus] = useState<'connecting' | 'connected' | 'error'>('connecting');

  const fetchSlots = useCallback(async () => {
    try {
      // Custom protocol: droplets://api/slots -> host="api", path="/slots"
      const response = await fetch('droplets://api/slots');
      const text = await response.text();
      console.log('Slots response:', text);
      const data = JSON.parse(text);
      if (data.type === 'slots' && data.data) {
        setSlots(data.data);
        setServerStatus('connected');
      } else if (data.error) {
        console.error('Slots error:', data.error);
        setServerStatus('error');
      }
    } catch (e) {
      console.error('Failed to fetch slots:', e);
      setServerStatus('error');
    }
  }, []);

  const fetchActivity = useCallback(async () => {
    try {
      const response = await fetch('droplets://api/activity');
      const data = await response.json();
      if (data.type === 'activity' && data.data) {
        setActivity(data.data);
      }
    } catch (e) {
      console.error('Failed to fetch activity:', e);
    }
  }, []);

  const startLearn = useCallback(async (slot: number) => {
    try {
      await fetch(`droplets://api/start_learn/${slot}`);
      fetchSlots(); // Refresh to show learning state
    } catch (e) {
      console.error('Failed to start learn:', e);
    }
  }, [fetchSlots]);

  const cancelLearn = useCallback(async () => {
    try {
      await fetch('droplets://api/cancel_learn');
      fetchSlots();
    } catch (e) {
      console.error('Failed to cancel learn:', e);
    }
  }, [fetchSlots]);

  useEffect(() => {
    // Initial fetch
    fetchSlots();
    fetchActivity();

    // Poll for updates
    const interval = setInterval(() => {
      fetchSlots();
      fetchActivity();
    }, 500);

    return () => clearInterval(interval);
  }, [fetchSlots, fetchActivity]);

  const formatTimestamp = (ts: number) => {
    const date = new Date(ts);
    return date.toLocaleTimeString('en-US', {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    });
  };

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1>Simply Droplets</h1>
          <span className="subtitle">AI Parameter Bridge</span>
        </div>
        <div className="server-status">
          <span className={`status-dot ${serverStatus}`}></span>
          <span className="status-text">MCP Server: localhost:9999</span>
        </div>
      </header>

      <main className="app-main">
        <section className="slots-section">
          <h2>Parameter Slots</h2>
          {slots.some(s => s.learning) && (
            <div className="learning-banner">
              <span>Waiting for MIDI CC input...</span>
              <button onClick={cancelLearn} className="cancel-learn-btn">Cancel</button>
            </div>
          )}
          <div className="slots-list">
            {slots.length === 0 ? (
              <div className="no-slots">
                <p>Loading parameter slots...</p>
              </div>
            ) : (
              slots.map((slot) => (
                <div key={slot.index} className={`slot-item ${slot.learning ? 'learning' : ''}`}>
                  <span className="slot-index">{slot.index}</span>
                  <span className="slot-name">{slot.name}</span>
                  <div className="slot-cc">
                    {slot.cc !== null ? (
                      <span className="cc-mapped">CC{slot.cc} Ch{slot.channel + 1}</span>
                    ) : (
                      <span className="cc-unmapped">unmapped</span>
                    )}
                  </div>
                  <div className="slot-bar-container">
                    <div
                      className="slot-bar"
                      style={{ width: `${slot.value * 100}%` }}
                    />
                  </div>
                  <span className="slot-value">{Math.round(slot.value * 100)}%</span>
                  <button
                    onClick={() => startLearn(slot.index)}
                    className={`map-btn ${slot.learning ? 'learning' : ''}`}
                    disabled={slot.learning}
                  >
                    {slot.learning ? 'Learning...' : 'Map'}
                  </button>
                </div>
              ))
            )}
          </div>
          <div className="slots-hint">
            <p>Click "Map" then send MIDI CC from your controller to assign it to a slot</p>
            <p>AI can set values via <code>set_param</code> - outputs the mapped CC</p>
          </div>
        </section>

        <section className="activity-section">
          <h2>Recent Activity</h2>
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
                  <span className="activity-cc">CC{event.cc}</span>
                  <span className="activity-value">{event.value}</span>
                  <span className="activity-channel">Ch{event.channel}</span>
                </div>
              ))
            )}
          </div>
        </section>
      </main>
    </div>
  );
};

export default App;
