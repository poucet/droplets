// SimplyVST message system for communication between frontend and Rust
(function() {
  const listeners = new Map();
  
  // Initialize namespace
  window.simplyvst = window.simplyvst || {};
  
  // Send message to plugin
  window.simplyvst.sendToPlugin = function(msg) {
    window.ipc.postMessage(JSON.stringify(msg));
  };
  
  // Add message listener
  window.simplyvst.addListener = function(messageType, callback) {
    if (!listeners.has(messageType)) {
      listeners.set(messageType, []);
    }
    listeners.get(messageType).push(callback);
    
    // Return unsubscribe function
    return function() {
      window.simplyvst.removeListener(messageType, callback);
    };
  };
  
  // Remove specific listener
  window.simplyvst.removeListener = function(messageType, callback) {
    const callbacks = listeners.get(messageType);
    if (callbacks) {
      const index = callbacks.indexOf(callback);
      if (index > -1) {
        callbacks.splice(index, 1);
      }
    }
  };
  
  // Remove all listeners for a message type
  window.simplyvst.removeAllListeners = function(messageType) {
    if (messageType) {
      listeners.delete(messageType);
    } else {
      listeners.clear();
    }
  };
  
  // Called from Rust via evaluate_script to send JSON to frontend
  window.simplyvst.receiveFromPlugin = function(msg) {
    try {
      const json = JSON.parse(msg);
      const messageType = json.type || 'default';
      
      // Call all registered listeners for this message type
      const callbacks = listeners.get(messageType);
      if (callbacks) {
        callbacks.forEach(callback => {
          try {
            callback(json.data || json, json);
          } catch (error) {
            console.error('Error in message listener:', error);
          }
        });
      }
      
      // Also call legacy handler if it exists
      if (window.onPluginMessage) {
        window.onPluginMessage(json);
      }
    } catch (error) {
      console.error('Error parsing plugin message:', error);
    }
  };
  
  // Legacy support - keep for backward compatibility
  window.sendToPlugin = window.simplyvst.sendToPlugin;
  window.onPluginMessage = null;
})();