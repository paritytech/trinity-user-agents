/**
 * The host on a phone's browser: full screen, where the browser offers it, and
 * an optional zoom lock. The page's own gestures are set in the stylesheet:
 * no pull to refresh and no double-tap zoom.
 */

const ZOOM_KEY = "truapi-web-signing-host:zoom-lock";
const VIEWPORT = "width=device-width, initial-scale=1, viewport-fit=cover";

export interface DeviceModeElements {
  /** The viewport meta element whose content the zoom lock sets. */
  viewport: HTMLMetaElement;
  /** Shown only where the browser lets a page go full screen. */
  fullscreen: HTMLButtonElement;
  zoomLock: HTMLInputElement;
  /** Says why full screen did not start, or what the browser offers instead. */
  status: HTMLElement;
}

/**
 * The viewport content for a locked or unlocked page zoom. Locked asks the
 * browser not to scale the page; a browser may ignore it, and Chrome on
 * Android does when its "Force enable zoom" setting is on.
 */
export function viewportContent(locked: boolean): string {
  return locked ? `${VIEWPORT}, maximum-scale=1, user-scalable=no` : VIEWPORT;
}

/** Apply the saved zoom lock, then bind the controls that change it and full screen. */
export function bindDeviceMode(elements: DeviceModeElements): void {
  const { viewport, fullscreen, zoomLock, status } = elements;

  const applyZoom = (locked: boolean): void => {
    viewport.content = viewportContent(locked);
    zoomLock.checked = locked;
  };
  applyZoom(localStorage.getItem(ZOOM_KEY) === "on");
  zoomLock.addEventListener("change", () => {
    if (zoomLock.checked) localStorage.setItem(ZOOM_KEY, "on");
    else localStorage.removeItem(ZOOM_KEY);
    applyZoom(zoomLock.checked);
  });

  const supported = document.fullscreenEnabled === true;
  fullscreen.hidden = !supported;
  if (!supported) {
    status.textContent =
      "This browser does not let a page go full screen. Add the page to the home screen to open it in its own window.";
    return;
  }
  const render = (): void => {
    const on = document.fullscreenElement !== null;
    fullscreen.textContent = on ? "Exit full screen" : "Full screen";
    fullscreen.setAttribute("aria-pressed", String(on));
  };
  fullscreen.addEventListener("click", () => {
    status.textContent = "";
    const change =
      document.fullscreenElement === null
        ? document.documentElement.requestFullscreen({ navigationUI: "hide" })
        : document.exitFullscreen();
    change.catch((error: unknown) => {
      status.textContent = `Full screen did not change: ${error instanceof Error ? error.message : String(error)}`;
    });
  });
  document.addEventListener("fullscreenchange", render);
  render();
}
