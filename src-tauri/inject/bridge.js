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
    // --- Outline DesktopBridge compatibility (from window.d.ts) ---

    platform: "tauri",

    version() {
      return invoke("get_version").catch(() => "0.1.0");
    },

    restart() {
      return invoke("restart_app");
    },

    restartAndInstall() {
      return invoke("restart_and_install");
    },

    checkForUpdates() {
      return invoke("check_for_updates");
    },

    onTitlebarDoubleClick() {
      return invoke("on_titlebar_double_click");
    },

    onLogout() {
      return invoke("on_logout");
    },

    addCustomHost(host) {
      return invoke("add_custom_host", { host });
    },

    setSpellCheckerLanguages(languages) {
      return invoke("set_spell_checker_languages", { languages });
    },

    setNotificationCount(count) {
      return invoke("set_notification_count", { count });
    },

    // Callback-based event registration (Outline registers callbacks via these)

    redirect(callback) {
      event.listen("redirect", (e) => {
        const { path, replace } = e.payload;
        callback(path, replace || false);
      });
    },

    updateDownloaded(callback) {
      event.listen("update-downloaded", callback);
    },

    focus(callback) {
      event.listen("window-focus", callback);
    },

    blur(callback) {
      event.listen("window-blur", callback);
    },

    openKeyboardShortcuts(callback) {
      event.listen("open-keyboard-shortcuts", callback);
    },

    goBack() {
      window.history.back();
    },

    goForward() {
      window.history.forward();
    },

    onNavigationStateChanged(callback) {
      const handler = () => {
        callback({
          canGoBack: window.history.length > 1,
          canGoForward: false,
        });
      };
      window.addEventListener("popstate", handler);
      handler();
      return () => window.removeEventListener("popstate", handler);
    },

    onFindInPage(callback) {
      event.listen("find-in-page", callback);
    },

    onReplaceInPage(callback) {
      event.listen("replace-in-page", callback);
    },

    getAutoLaunch() {
      return invoke("get_auto_launch");
    },

    setAutoLaunch(enabled) {
      return invoke("set_auto_launch", { enabled });
    },

    // --- Tauri extensions ---

    getServerList() {
      return invoke("get_server_list");
    },

    addServer(config) {
      return invoke("add_server", { config });
    },

    removeServer(id) {
      return invoke("remove_server", { id });
    },

    switchServer(id) {
      return invoke("switch_server", { id });
    },

    getOfflineStatus() {
      return invoke("get_offline_status");
    },

    syncNow() {
      return invoke("sync_now");
    },

    proxyFetch(method, path, headers, body) {
      return invoke("proxy_fetch", { method, path, headers, body });
    },
  };
})();
