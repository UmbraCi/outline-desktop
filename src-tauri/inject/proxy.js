// Injected into the WebView before page load.
// Overrides window.fetch to route API requests through Tauri backend.
// Also blocks Service Worker registration to prevent SW from intercepting requests.

(function () {
  // Block remote Service Worker registration.
  // The Tauri backend controls offline caching; a remote SW would intercept
  // requests at the network layer and bypass our fetch proxy.
  Object.defineProperty(navigator, "serviceWorker", {
    value: undefined,
    writable: false,
  });

  const core = window.__TAURI__?.core;
  if (!core) {
    console.error("[TauriProxy] __TAURI__.core not available");
    return;
  }

  const { invoke } = core;
  const originalFetch = window.fetch.bind(window);

  // --- Fetch proxy for /api/* ---

  window.fetch = async function (input, init) {
    const url = typeof input === "string" ? input : input.url;

    // Only intercept API requests
    if (url.startsWith("/api/")) {
      const method = init?.method || "GET";
      const headers = {};
      if (init?.headers) {
        const h =
          init.headers instanceof Headers
            ? Object.fromEntries(init.headers.entries())
            : init.headers;
        Object.assign(headers, h);
      }
      const body = init?.body || null;

      try {
        const response = await invoke("proxy_fetch", {
          method,
          path: url,
          headers,
          body: body ? String(body) : null,
        });

        const responseBody = response.body || "";
        const responseHeaders = new Headers(response.headers || {});
        return new Response(responseBody, {
          status: response.status,
          statusText: response.status === 200 ? "OK" : "Error",
          headers: responseHeaders,
        });
      } catch (err) {
        console.error("[TauriProxy] fetch error:", err);
        return new Response(
          JSON.stringify({ error: "Offline or proxy error" }),
          { status: 503, headers: { "Content-Type": "application/json" } }
        );
      }
    }

    // Non-API requests: use original fetch
    return originalFetch(input, init);
  };

  // TODO: WebSocket proxy will be implemented in Task 7 with a local WS server.
  // Tauri IPC is request/response based and cannot proxy raw WebSocket connections,
  // so WebSocket connections currently use direct connection (original behavior).
})();
