import React, { useEffect, useState, useCallback } from 'react';
import './App.css';

interface SlotInfo {
  index: number;
  name: string;
  value: number;
}

interface ActivityEvent {
  timestamp: number;
  instance: string;
  slot: number;
  value: number;
}

const App: React.FC = () => {
  const [slots, setSlots] = useState<SlotInfo[]>([]);
  const [activity, setActivity] = useState<ActivityEvent[]>([]);
  const [serverStatus, setServerStatus] = useState<'connecting' | 'connected' | 'error'>('connecting');

  const fetchSlots = useCallback(async () => {
    try {
      const response = await fetch('droplets://api/slots');
      const text = await response.text();
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

  useEffect(() => {
    fetchSlots();
    fetchActivity();

    const interval = setInterval(() => {
      fetchSlots();
      fetchActivity();
    }, 100); // Faster polling for smoother parameter updates

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
          <h2>Automatable Parameters</h2>
          <div className="slots-list">
            {slots.length === 0 ? (
              <div className="no-slots">
                <p>Loading parameter slots...</p>
              </div>
            ) : (
              slots.map((slot) => (
                <div key={slot.index} className="slot-item">
                  <span className="slot-index">{slot.index}</span>
                  <span className="slot-name">{slot.name}</span>
                  <div className="slot-bar-container">
                    <div
                      className="slot-bar"
                      style={{ width: `${slot.value * 100}%` }}
                    />
                  </div>
                  <span className="slot-value">{Math.round(slot.value * 100)}%</span>
                </div>
              ))
            )}
          </div>
          <div className="slots-hint">
            <p><strong>How to use:</strong></p>
            <p>1. In your DAW, map these parameters to any plugin using modulation</p>
            <p>2. AI sets values via MCP <code>set_param</code> tool</p>
            <p>3. DAW routes the parameter changes to your target plugin</p>
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
                  <span className="activity-slot">Slot {event.slot}</span>
                  <span className="activity-value">{Math.round(event.value * 100)}%</span>
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
