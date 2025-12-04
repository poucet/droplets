/**
 * API Client - Unified interface for plugin (IPC) and standalone (HTTP/WebSocket) modes
 *
 * In plugin mode (wry webview): Uses droplets:// protocol and IPCWebSocket shim
 * In standalone mode (browser): Uses http://localhost:9998/api/ and native WebSocket
 */

import type {
  SlotsResponse,
  ActivityResponse,
  FuguesResponse,
  TransportResponse,
  FugueResponse,
  InstancesResponse,
  OkResponse,
  TransportState,
  TimedFugueEvent,
  LoopMode,
  QuantizeMode,
  CancelMode,
} from '../types';

// Extend Window interface for simplyvst
declare global {
  interface Window {
    simplyvst?: {
      isPluginMode: boolean;
      apiBase: string;
      WebSocket: typeof WebSocket;
    };
  }
}

// =============================================================================
// Mode detection and configuration - done ONCE at module load
// =============================================================================

const isPluginMode = typeof window !== 'undefined' && window.simplyvst?.isPluginMode === true;

// Configure based on mode
const API_BASE = isPluginMode ? 'droplets://api' : 'http://localhost:9998/api';
const WS_URL = isPluginMode ? '' : 'ws://localhost:9998/ws';
const WS_CLASS = isPluginMode ? window.simplyvst!.WebSocket : WebSocket;
const SHOULD_RECONNECT = !isPluginMode; // Only reconnect in standalone mode

// Build URL with optional instance param (standalone mode only needs it)
const buildUrl = (path: string, instance: string): string => {
  if (isPluginMode) {
    return `${API_BASE}${path}`;
  }
  return `${API_BASE}${path}?instance=${encodeURIComponent(instance)}`;
};

// Build WebSocket URL with instance param
const buildWsUrl = (instance: string): string => {
  if (isPluginMode) {
    return ''; // IPCWebSocket doesn't need a URL
  }
  return `${WS_URL}?instance=${encodeURIComponent(instance)}`;
};

// =============================================================================
// REST API Functions
// =============================================================================

async function apiFetch<T>(path: string, instance = 'default'): Promise<T> {
  const response = await fetch(buildUrl(path, instance));
  if (!response.ok) {
    throw new Error(`API error: ${response.status}`);
  }
  return response.json();
}

export async function getInstances(): Promise<InstancesResponse> {
  return apiFetch<InstancesResponse>('/instances');
}

export async function getSlots(instance = 'default'): Promise<SlotsResponse> {
  return apiFetch<SlotsResponse>('/slots', instance);
}

export async function getActivity(): Promise<ActivityResponse> {
  return apiFetch<ActivityResponse>('/activity');
}

export async function getFugues(instance = 'default'): Promise<FuguesResponse> {
  return apiFetch<FuguesResponse>('/fugues', instance);
}

export async function getTransport(instance = 'default'): Promise<TransportResponse> {
  return apiFetch<TransportResponse>('/transport', instance);
}

export async function getFugue(id: number, instance = 'default'): Promise<FugueResponse> {
  return apiFetch<FugueResponse>(`/fugue/${id}`, instance);
}

export async function startLearn(slot: number, instance = 'default'): Promise<OkResponse> {
  return apiFetch<OkResponse>(`/start_learn/${slot}`, instance);
}

export async function cancelLearn(instance = 'default'): Promise<OkResponse> {
  return apiFetch<OkResponse>('/cancel_learn', instance);
}

export async function wiggleSlot(slot: number, instance = 'default'): Promise<OkResponse> {
  return apiFetch<OkResponse>(`/wiggle/${slot}`, instance);
}

export async function noteOn(note: number, velocity = 100, instance = 'default'): Promise<OkResponse> {
  return apiFetch<OkResponse>(`/note_on/${note}/${velocity}`, instance);
}

export async function noteOff(note: number, instance = 'default'): Promise<OkResponse> {
  return apiFetch<OkResponse>(`/note_off/${note}`, instance);
}

// =============================================================================
// Fugue Queue/Cancel API
// =============================================================================

export interface QueueFugueRequest {
  tag: string | null;
  events: TimedFugueEvent[];
  duration_beats: number;
  loop_mode: LoopMode;
  quantize: QuantizeMode;
  cancel_mode: CancelMode;
}

export interface QueueFugueResponse {
  ok: boolean;
  fugue_id?: bigint;
  error?: string;
}

async function apiPost<T, R>(path: string, body: T, instance = 'default'): Promise<R> {
  const url = buildUrl(path, instance);
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`API error: ${response.status}`);
  }
  return response.json();
}

export async function queueFugue(fugue: QueueFugueRequest, instance = 'default'): Promise<QueueFugueResponse> {
  return apiPost<QueueFugueRequest, QueueFugueResponse>('/queue_fugue', fugue, instance);
}

export async function cancelFugue(id: bigint, instance = 'default'): Promise<OkResponse> {
  // Convert bigint to number for JSON serialization (safe for fugue IDs which are u64 but in practice small)
  return apiPost<{ id: number }, OkResponse>('/cancel_fugue', { id: Number(id) }, instance);
}

export async function cancelFuguesByTag(tag: string, instance = 'default'): Promise<OkResponse> {
  return apiPost<{ tag: string }, OkResponse>('/cancel_fugues_by_tag', { tag }, instance);
}

export async function clearFugues(instance = 'default'): Promise<OkResponse> {
  return apiFetch<OkResponse>('/clear_fugues', instance);
}

// =============================================================================
// WebSocket for Real-time Updates
// =============================================================================

export interface WsTransportMessage {
  type: 'transport';
  transport: TransportState;
}

export interface WsFuguesMessage {
  type: 'fugues';
  infos: FuguesResponse['infos'];
  definitions: FuguesResponse['definitions'];
}

export type WsMessage = WsTransportMessage | WsFuguesMessage;

export interface RealtimeCallbacks {
  onTransport?: (transport: TransportState) => void;
  onFugues?: (response: FuguesResponse) => void;
  onConnect?: () => void;
  onDisconnect?: () => void;
  onError?: (error: Error) => void;
}

/**
 * Real-time connection for transport and fugue updates.
 * Uses native WebSocket in standalone mode, IPCWebSocket shim in plugin mode.
 */
export class RealtimeConnection {
  private ws: WebSocket | null = null;
  private callbacks: RealtimeCallbacks;
  private instance: string;
  private reconnectTimer: number | null = null;
  private isDestroyed = false;

  constructor(callbacks: RealtimeCallbacks, instance = 'default') {
    this.callbacks = callbacks;
    this.instance = instance;
  }

  connect(): void {
    if (this.isDestroyed) return;

    this.ws = new WS_CLASS(buildWsUrl(this.instance));

    this.ws.onopen = () => {
      this.callbacks.onConnect?.();
    };

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as WsMessage;
        if (msg.type === 'transport') {
          this.callbacks.onTransport?.(msg.transport);
        } else if (msg.type === 'fugues') {
          this.callbacks.onFugues?.({
            infos: msg.infos,
            definitions: msg.definitions,
          });
        }
      } catch (e) {
        console.error('Failed to parse realtime message:', e);
      }
    };

    this.ws.onclose = () => {
      this.callbacks.onDisconnect?.();
      this.ws = null;

      // Reconnect after 1 second (standalone mode only)
      if (!this.isDestroyed && SHOULD_RECONNECT) {
        this.reconnectTimer = window.setTimeout(() => this.connect(), 1000);
      }
    };

    this.ws.onerror = () => {
      this.callbacks.onError?.(new Error('Realtime connection error'));
    };
  }

  disconnect(): void {
    this.isDestroyed = true;

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }

    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  setInstance(instance: string): void {
    if (this.instance === instance) return;
    this.instance = instance;

    // Reconnect with new instance (standalone mode only - triggers via close)
    if (this.ws && SHOULD_RECONNECT) {
      this.ws.close();
    }
  }
}
