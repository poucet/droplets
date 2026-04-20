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
  ProjectLayout,
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
  // Thread `instance` through the query string in both modes. Plugin
  // mode previously dropped it, which meant every API call hit the
  // webview-owner plugin instance regardless of what the UI dropdown
  // said — so switching instance did nothing. The wry protocol handler
  // now parses this back out.
  return `${API_BASE}${path}?instance=${encodeURIComponent(instance)}`;
};

// Build WebSocket URL with instance param
const buildWsUrl = (): string => {
  if (isPluginMode) {
    return ''; // IPCWebSocket doesn't need a URL
  }
  // No instance filter — one subscription covers every connected instance,
  // tagged per message. The UI routes based on `instance_id` in each payload.
  return WS_URL;
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

export async function getSelf(): Promise<{ id: string }> {
  return apiFetch<{ id: string }>('/self');
}

export async function getInstances(): Promise<InstancesResponse> {
  return apiFetch<InstancesResponse>('/instances');
}

export async function renameInstance(instance: string, name: string): Promise<OkResponse> {
  return apiPost<{ instance: string; name: string }, OkResponse>('/rename_instance', { instance, name });
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

/**
 * Fetch the current DAW project layout — what the host controller
 * extension last pushed. Returns an empty `{ tracks: [] }` when no
 * extension is running. Used for the initial render of the DAW tab;
 * subsequent updates flow through the WebSocket.
 */
export async function getProjectLayout(): Promise<ProjectLayout> {
  return apiFetch<ProjectLayout>('/project_layout');
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

/** Add a new CC slot on the given instance. Returns the new slot's index. */
export async function addSlot(cc: number, name: string, instance = 'default'): Promise<{ ok: boolean; index: number }> {
  return apiPost<{ cc: number; name: string }, { ok: boolean; index: number }>('/slots', { cc, name }, instance);
}

/** Remove a slot by index. */
export async function removeSlot(slot: number, instance = 'default'): Promise<OkResponse> {
  const url = buildUrl(`/slots/${slot}`, instance);
  const response = await fetch(url, { method: 'DELETE' });
  if (!response.ok) throw new Error(`API error: ${response.status}`);
  return response.json();
}

/** Change the CC number backing a slot. */
export async function setSlotCc(slot: number, cc: number, instance = 'default'): Promise<OkResponse> {
  return apiPost<{ cc: number }, OkResponse>(`/slots/${slot}/cc`, { cc }, instance);
}

/** Rename a slot. */
export async function renameSlotApi(slot: number, name: string, instance = 'default'): Promise<OkResponse> {
  return apiPost<{ name: string }, OkResponse>(`/slots/${slot}/name`, { name }, instance);
}

/** Change the MIDI channel for a slot (0-15). */
export async function setSlotChannel(slot: number, channel: number, instance = 'default'): Promise<OkResponse> {
  return apiPost<{ channel: number }, OkResponse>(`/slots/${slot}/channel`, { channel }, instance);
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
  fugue_id?: string;
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

export async function cancelFugue(id: string, instance = 'default'): Promise<OkResponse> {
  return apiPost<{ id: string }, OkResponse>('/cancel_fugue', { id }, instance);
}

export async function cancelFuguesByTag(tag: string, instance = 'default'): Promise<OkResponse> {
  return apiPost<{ tag: string }, OkResponse>('/cancel_fugues_by_tag', { tag }, instance);
}

export async function clearFugues(instance = 'default'): Promise<OkResponse> {
  return apiFetch<OkResponse>('/clear_fugues', instance);
}

// =============================================================================
// Settings API
// =============================================================================

export interface SettingsResponse {
  export_path: string;
  mcp_port: number;
  mcp_url: string;
  custom_instructions: string;
}

export async function getSettings(): Promise<SettingsResponse> {
  return apiFetch<SettingsResponse>('/settings');
}

export interface UpdateSettingsRequest {
  export_path?: string;
  custom_instructions?: string;
}

export async function updateSettings(settings: UpdateSettingsRequest): Promise<OkResponse> {
  return apiPost<UpdateSettingsRequest, OkResponse>('/settings', settings);
}

export async function revealExports(): Promise<OkResponse> {
  return apiFetch<OkResponse>('/reveal_exports');
}

// =============================================================================
// Fugue Export API
// =============================================================================

export interface ExportFugueRequest {
  id: string;
  tempo?: number;
}

export interface ExportFugueResponse {
  ok: boolean;
  path?: string;
  error?: string;
}

export async function exportFugue(id: string, tempo?: number, instance = 'default'): Promise<ExportFugueResponse> {
  return apiPost<ExportFugueRequest, ExportFugueResponse>('/export_fugue', { id, tempo }, instance);
}

// =============================================================================
// WebSocket for Real-time Updates
// =============================================================================

export interface WsTransportMessage {
  type: 'transport';
  instance_id: string;
  transport: TransportState;
}

export interface WsFuguesMessage {
  type: 'fugues';
  instance_id: string;
  infos: FuguesResponse['infos'];
  definitions: FuguesResponse['definitions'];
}

/**
 * Project layout pushed from the backend whenever the host controller
 * extension (e.g. the Bitwig extension) sends a new snapshot. Flows
 * through broadcast so the UI reflects DAW changes without polling.
 */
export interface WsProjectLayoutMessage {
  type: 'project_layout';
  tracks: ProjectLayout['tracks'];
}

export type WsMessage = WsTransportMessage | WsFuguesMessage | WsProjectLayoutMessage;

/**
 * Callbacks for RealtimeConnection events. Transport and fugue callbacks
 * receive the `instance_id` so the UI can route the update to its
 * per-instance state map — one subscription, all instances, no reconnect
 * on instance switch.
 */
export interface RealtimeCallbacks {
  onTransport?: (instanceId: string, transport: TransportState) => void;
  onFugues?: (instanceId: string, response: FuguesResponse) => void;
  onProjectLayout?: (layout: ProjectLayout) => void;
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
  private reconnectTimer: number | null = null;
  private isDestroyed = false;

  constructor(callbacks: RealtimeCallbacks) {
    this.callbacks = callbacks;
  }

  connect(): void {
    if (this.isDestroyed) return;

    this.ws = new WS_CLASS(buildWsUrl());

    this.ws.onopen = () => {
      this.callbacks.onConnect?.();
    };

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as WsMessage;
        if (msg.type === 'transport') {
          this.callbacks.onTransport?.(msg.instance_id, msg.transport);
        } else if (msg.type === 'fugues') {
          this.callbacks.onFugues?.(msg.instance_id, {
            infos: msg.infos,
            definitions: msg.definitions,
          });
        } else if (msg.type === 'project_layout') {
          console.debug(
            '[droplets] WS project_layout:',
            msg.tracks.length, 'tracks'
          );
          this.callbacks.onProjectLayout?.({ tracks: msg.tracks });
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
}
