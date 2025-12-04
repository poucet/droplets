// SimplyVST message system for communication between frontend and Rust
(function() {
  const listeners = new Map();

  // Initialize namespace
  window.simplyvst = window.simplyvst || {};

  // Declare that we're running in plugin mode (injected by wry)
  window.simplyvst.isPluginMode = true;

  // API base URL for fetch requests (custom protocol in plugin)
  window.simplyvst.apiBase = 'droplets://api';

  // ==========================================================================
  // WebSocket-like shim for IPC communication
  // ==========================================================================
  // This allows the frontend to use the same WebSocket API in both plugin
  // and standalone mode. In plugin mode, we receive push messages from Rust
  // via evaluate_script calling window.simplyvst._onRealtimeMessage().

  // Active IPCWebSocket instances that should receive messages
  const activeConnections = new Set();

  class IPCWebSocket {
    constructor(url) {
      this.url = url;
      this.readyState = 0; // CONNECTING
      this.onopen = null;
      this.onmessage = null;
      this.onclose = null;
      this.onerror = null;

      // Register this connection
      activeConnections.add(this);

      // Connect on next tick (like real WebSocket)
      setTimeout(() => this._connect(), 0);
    }

    _connect() {
      this.readyState = 1; // OPEN
      if (this.onopen) {
        this.onopen({ type: 'open' });
      }
    }

    // Called when we receive a message from Rust
    _receiveMessage(data) {
      if (this.readyState !== 1) return;
      if (this.onmessage) {
        this.onmessage({
          type: 'message',
          data: typeof data === 'string' ? data : JSON.stringify(data)
        });
      }
    }

    send(data) {
      // Send to plugin via IPC
      if (window.ipc) {
        window.ipc.postMessage(JSON.stringify({ type: 'ws_message', data }));
      }
    }

    close() {
      activeConnections.delete(this);
      this.readyState = 3; // CLOSED
      if (this.onclose) {
        this.onclose({ type: 'close' });
      }
    }
  }

  // Expose the shim
  window.simplyvst.WebSocket = IPCWebSocket;

  // Called by Rust via evaluate_script to push realtime messages
  window.simplyvst._onRealtimeMessage = function(data) {
    const jsonStr = typeof data === 'string' ? data : JSON.stringify(data);
    for (const conn of activeConnections) {
      conn._receiveMessage(jsonStr);
    }
  };

  // Convenience: Push transport update
  window.simplyvst._pushTransport = function(transport) {
    window.simplyvst._onRealtimeMessage({
      type: 'transport',
      transport: transport
    });
  };

  // Convenience: Push fugues update
  window.simplyvst._pushFugues = function(infos, definitions) {
    window.simplyvst._onRealtimeMessage({
      type: 'fugues',
      infos: infos,
      definitions: definitions
    });
  };
})();
