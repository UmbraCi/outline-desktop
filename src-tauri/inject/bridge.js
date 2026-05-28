// Injected into the WebView before page load.
// Creates window.TauriBridge compatible with Outline's DesktopBridge interface.
//
// Outline expects DesktopBridge methods that register callbacks for events
// (redirect, focus, blur, updateDownloaded, openKeyboardShortcuts).
// In Tauri, we use window.__TAURI__.event.listen() for these.

(function () {
  const core = window.__TAURI__?.core;
  const event = window.__TAURI__?.event;

  if (!core || !event) {
    console.error("[TauriBridge] __TAURI__ not available");
    return;
  }

  const { invoke } = core;

  window.TauriBridge = {
    // --- Outline DesktopBridge compatibility ---

    // Register a callback for navigation redirects from the main process.
    // Callback receives (path: string, replace: boolean).
    redirect(callback) {
      event.listen("redirect", (event) => {
        const { path, replace } = event.payload;
        callback(path, replace || false);
      });
    },

    // Register a callback for when an update has been downloaded.
    updateDownloaded(callback) {
      event.listen("update-downloaded", callback);
    },

    // Trigger restart and install of a pending update.
    restartAndInstall() {
      return invoke("restart_and_install");
    },

    // Register a callback for window focus events.
    focus(callback) {
      event.listen("window-focus", callback);
    },

    // Register a callback for window blur events.
    blur(callback) {
      event.listen("window-blur", callback);
    },

    // Register a callback for the "open keyboard shortcuts" action.
    openKeyboardShortcuts(callback) {
      event.listen("open-keyboard-shortcuts", callback);
    },

    // Get whether the app launches at login.
    getAutoLaunch() {
      return invoke("get_auto_launch");
    },

    // Set whether the app launches at login.
    setAutoLaunch(enabled) {
      return invoke("set_auto_launch", { enabled });
    },

    // --- Tauri extensions ---

    // Get the list of configured servers.
    getServerList() {
      return invoke("get_server_list");
    },

    // Add a new server configuration.
    addServer(config) {
      return invoke("add_server", { config });
    },

    // Remove a server configuration by ID.
    removeServer(id) {
      return invoke("remove_server", { id });
    },

    // Switch to a different server.
    switchServer(id) {
      return invoke("switch_server", { id });
    },

    // Get current offline/sync status.
    getOfflineStatus() {
      return invoke("get_offline_status");
    },

    // Trigger an immediate sync.
    syncNow() {
      return invoke("sync_now");
    },

    // Proxy a fetch request through the Tauri backend.
    proxyFetch(method, path, headers, body) {
      return invoke("proxy_fetch", { method, path, headers, body });
    },
  };
})();
