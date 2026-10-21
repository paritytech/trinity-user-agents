(() => {
  if (window !== window.top || globalThis.__nativeTelemetryAcceptance) return;

  const marker = "__NATIVE_TELEMETRY_MARKER__";
  const passThrough = __NATIVE_TELEMETRY_PASS_THROUGH__;
  const envelopes = [];
  let markerTriggered = false;
  const originalFetch = globalThis.fetch.bind(globalThis);
  const sentryEnvelope = (url) => {
    try {
      const parsed = new URL(url, location.href);
      return /\/api\/\d+\/envelope\//.test(parsed.pathname) && /(^|\.)sentry\.io$/.test(parsed.hostname);
    } catch {
      return false;
    }
  };
  const bodyText = async (input, init) => {
    const body = init?.body;
    if (typeof body === "string") return body;
    if (body instanceof Blob) return body.text();
    if (body instanceof ArrayBuffer) return new TextDecoder().decode(body);
    if (ArrayBuffer.isView(body)) return new TextDecoder().decode(body);
    if (input instanceof Request) return input.clone().text();
    return "";
  };

  globalThis.fetch = async (input, init) => {
    const url = typeof input === "string" || input instanceof URL ? String(input) : input.url;
    if (!sentryEnvelope(url)) return originalFetch(input, init);
    try {
      const body = await bodyText(input, init);
      if (body) {
        if (envelopes.length >= 256) envelopes.shift();
        envelopes.push(body);
        // Session startup records session.link immediately before this log.
        // Trigger now so startup chatter cannot evict that breadcrumb.
        if (!markerTriggered && body.includes("previous-session-end")) {
          markerTriggered = true;
          setTimeout(() => {
            throw new Error(marker);
          }, 0);
        }
      }
    } catch {
      // The acceptance hook must not affect the page if a body cannot be copied.
    }
    return passThrough ? originalFetch(input, init) : new Response("", { status: 200 });
  };

  const snapshot = () => {
    const descriptor = Object.getOwnPropertyDescriptor(globalThis, "__trinityHostTelemetry");
    const bootstrap = globalThis.__trinityHostTelemetry;
    return {
      href: location.href,
      bootstrap: bootstrap && typeof bootstrap === "object" ? { ...bootstrap } : null,
      descriptor: {
        frozen: !!bootstrap && Object.isFrozen(bootstrap),
        writable: descriptor?.writable ?? null,
        configurable: descriptor?.configurable ?? null,
      },
      envelopes: envelopes.slice(),
    };
  };

  Object.defineProperty(globalThis, "__nativeTelemetryAcceptance", {
    value: Object.freeze({
      snapshot,
    }),
    configurable: false,
    writable: false,
  });
})();
