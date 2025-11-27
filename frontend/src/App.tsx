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
  channel: number;
  cc: number;
  value: number;
}

interface IpcMessage {
  type: string;
  data?: SlotInfo[] | ActivityEvent[];
}

declare global {
  interface Window {
    ipc: {
      postMessage: (message: string) => void;
    };
  }
}

const App: React.FC = () => {
  const [slots, setSlots] = useState<SlotInfo[]>([]);
  const [activity, setActivity] = useState<ActivityEvent[]>([]);
  const [serverStatus, setServerStatus] = useState<'connecting' | 'connected' | 'offline'>('connecting');

  const requestData = useCallback(() => {
    if (window.ipc) {
      window.ipc.postMessage(JSON.stringify({ type: 'get_slots' }));
      window.ipc.postMessage(JSON.stringify({ type: 'get_activity' }));
    }
  }, []);

  useEffect(() => {
    const handleMessage = (event: MessageEvent) => {
      try {
        const message: IpcMessage = JSON.parse(event.data);
        if (message.type === 'slots' && message.data) {
          setSlots(message.data as SlotInfo[]);
          setServerStatus('connected');
        } else if (message.type === 'activity' && message.data) {
          setActivity(message.data as ActivityEvent[]);
          setServerStatus('connected');
        }
      } catch (e) {
        console.error('Failed to parse IPC message:', e);
      }
    };

    window.addEventListener('message', handleMessage);

    // Poll for updates
    const interval = setInterval(requestData, 500);
    requestData();

    // Set connected after initial request
    setTimeout(() => {
      if (serverStatus === 'connecting') {
        setServerStatus('connected');
      }
    }, 1000);

    return () => {
      window.removeEventListener('message', handleMessage);
      clearInterval(interval);
    };
  }, [requestData, serverStatus]);

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
            <p>Map these slots to other plugin parameters using DAW modulation</p>
            <p>AI can rename slots via <code>rename_slot</code> tool</p>
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
