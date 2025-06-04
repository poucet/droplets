import React, { useEffect, useState } from 'react';
import './App.css';

interface PluginParams {
  grain_size: number;
  density: number;
  time_warp: number;
  spatial_spread: number;
  dry_wet: number;
}

interface IpcMessage {
  type: string;
  id?: string;
  value?: number;
  grain_size?: number;
  density?: number;
  time_warp?: number;
  spatial_spread?: number;
  dry_wet?: number;
}

declare global {
  interface Window {
    ipc: {
      postMessage: (message: string) => void;
    };
  }
}

const App: React.FC = () => {
  const [params, setParams] = useState<PluginParams>({
    grain_size: 1024,
    density: 10.0,
    time_warp: 1.0,
    spatial_spread: 1.0,
    dry_wet: 1.0,
  });

  useEffect(() => {
    // Request all parameters on startup
    if (window.ipc) {
      const getAllParamsMessage: IpcMessage = {
        type: 'GetAllParameters'
      };
      window.ipc.postMessage(JSON.stringify(getAllParamsMessage));
    }

    // Set up message listener for IPC responses
    const handleMessage = (event: MessageEvent) => {
      try {
        const message: IpcMessage = JSON.parse(event.data);
        if (message.type === 'AllParameters') {
          setParams({
            grain_size: message.grain_size || 1024,
            density: message.density || 10.0,
            time_warp: message.time_warp || 1.0,
            spatial_spread: message.spatial_spread || 1.0,
            dry_wet: message.dry_wet || 1.0,
          });
        } else if (message.type === 'ParameterChanged' && message.id && message.value !== undefined) {
          setParams(prev => ({ ...prev, [message.id!]: message.value! }));
        }
      } catch (e) {
        console.error('Failed to parse IPC message:', e);
      }
    };

    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, []);

  const handleParameterChange = (id: keyof PluginParams, value: number) => {
    setParams(prev => ({ ...prev, [id]: value }));
    if (window.ipc) {
      const setParamMessage: IpcMessage = {
        type: 'SetParameter',
        id: id,
        value: value
      };
      window.ipc.postMessage(JSON.stringify(setParamMessage));
    }
  };

  const createSlider = (
    id: keyof PluginParams,
    label: string,
    min: number,
    max: number,
    step: number = 0.01,
    suffix: string = ''
  ) => (
    <div className="parameter-row">
      <label className="parameter-label">{label}</label>
      <div className="parameter-control">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={params[id]}
          onChange={(e) => handleParameterChange(id, parseFloat(e.target.value))}
          className="parameter-slider"
        />
        <span className="parameter-value">
          {id === 'grain_size' ? Math.round(params[id]) : params[id].toFixed(2)}
          {suffix}
        </span>
      </div>
    </div>
  );

  return (
    <div className="app">
      <header className="app-header">
        <h1>Simply Droplets</h1>
        <span className="subtitle">3D Granular Synthesis</span>
      </header>
      
      <main className="app-main">
        <section className="parameters-section">
          <h2>Parameters</h2>
          <div className="parameters-grid">
            {createSlider('grain_size', 'Grain Size', 64, 8192, 1, ' samples')}
            {createSlider('density', 'Density', 0.1, 100.0, 0.1, ' /s')}
            {createSlider('time_warp', 'Time Warp', 0.1, 4.0)}
            {createSlider('spatial_spread', 'Spatial Spread', 0.0, 1.0, 0.01, '%')}
            {createSlider('dry_wet', 'Dry/Wet', 0.0, 1.0, 0.01, '%')}
          </div>
        </section>

        <section className="visualization-section">
          <h2>3D Droplet Visualization</h2>
          <div className="visualization-placeholder">
            <p>3D visualization will be implemented here</p>
            <p>Showing droplet positions and movement in real-time</p>
          </div>
        </section>
      </main>
    </div>
  );
};

export default App;