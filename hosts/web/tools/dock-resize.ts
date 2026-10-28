/** The least the developer tools can shrink to, at the bottom and on the right. */
const MIN_HEIGHT = 160;
const MIN_WIDTH = 320;
/** What the app keeps beside or above the tools. The stylesheet keeps the same. */
const APP_HEIGHT = 120;
const APP_WIDTH = 360;
const KEY_STEP = 24;
const HEIGHT_KEY = "truapi-web-signing-host:devtools-height";
const WIDTH_KEY = "truapi-web-signing-host:devtools-width";

/**
 * Let the inner edge of the developer tools be dragged, or moved with the
 * keyboard, to set their size: the top edge sets their height at the bottom,
 * the left edge their width on the right. Each size is kept for the next
 * visit, apart from the other.
 *
 * Each size is one CSS variable, so nothing is moved or re-created and the
 * product frame keeps running. The stylesheet clamps both to the space the
 * window has, also when the window changes size later.
 */
export function bindDockResize(
  handle: HTMLElement,
  dock: HTMLElement,
  storage: Pick<Storage, "getItem" | "setItem"> = localStorage,
): void {
  const right = (): boolean => document.body.dataset.devSide === "right";
  const limits = (): [number, number] =>
    right()
      ? [MIN_WIDTH, Math.max(MIN_WIDTH, window.innerWidth - APP_WIDTH)]
      : [MIN_HEIGHT, Math.max(MIN_HEIGHT, window.innerHeight - APP_HEIGHT)];
  const current = (): number => {
    const box = dock.getBoundingClientRect();
    return Math.round(right() ? box.width : box.height);
  };
  const apply = (side: "right" | "bottom", size: number): void => {
    if (side === "right")
      document.body.style.setProperty("--devtools-w", `${size}px`);
    else dock.style.setProperty("--devtools-h", `${size}px`);
  };

  /** Say which edge this is and where it stands, for assistive technology. */
  const describe = (): void => {
    const [min, max] = limits();
    handle.setAttribute(
      "aria-orientation",
      right() ? "vertical" : "horizontal",
    );
    handle.setAttribute(
      "aria-label",
      right() ? "Developer tools width" : "Developer tools height",
    );
    handle.setAttribute("aria-valuemin", String(min));
    handle.setAttribute("aria-valuemax", String(max));
    handle.setAttribute("aria-valuenow", String(current()));
  };

  const set = (size: number): void => {
    const [min, max] = limits();
    const clamped = Math.round(Math.min(max, Math.max(min, size)));
    apply(right() ? "right" : "bottom", clamped);
    storage.setItem(right() ? WIDTH_KEY : HEIGHT_KEY, String(clamped));
    handle.setAttribute("aria-valuenow", String(clamped));
  };

  for (const [side, key] of [
    ["right", WIDTH_KEY],
    ["bottom", HEIGHT_KEY],
  ] as const) {
    const saved = Number(storage.getItem(key));
    if (Number.isFinite(saved) && saved > 0) apply(side, saved);
  }

  /** The pointer that drags, where it went down and the size then, so the edge does not jump to it. */
  let dragging: number | null = null;
  let start = { at: 0, size: 0 };
  const end = (): void => {
    if (dragging === null) return;
    if (handle.hasPointerCapture(dragging))
      handle.releasePointerCapture(dragging);
    dragging = null;
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    describe();
  };
  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    dragging = event.pointerId;
    start = { at: right() ? event.clientX : event.clientY, size: current() };
    handle.setPointerCapture(event.pointerId);
    document.body.style.userSelect = "none";
    document.body.style.cursor = right() ? "col-resize" : "row-resize";
    event.preventDefault();
  });
  handle.addEventListener("pointermove", (event) => {
    if (dragging !== event.pointerId) return;
    // The tools sit at the right or bottom, so moving left or up grows them.
    set(start.size + start.at - (right() ? event.clientX : event.clientY));
  });
  handle.addEventListener("pointerup", end);
  handle.addEventListener("pointercancel", end);
  handle.addEventListener("lostpointercapture", end);
  handle.addEventListener("keydown", (event) => {
    const [min, max] = limits();
    const grow = right() ? "ArrowLeft" : "ArrowUp";
    const shrink = right() ? "ArrowRight" : "ArrowDown";
    const next =
      event.key === grow
        ? current() + KEY_STEP
        : event.key === shrink
          ? current() - KEY_STEP
          : event.key === "Home"
            ? min
            : event.key === "End"
              ? max
              : null;
    if (next === null) return;
    event.preventDefault();
    set(next);
  });
  handle.addEventListener("focus", describe);
  window.addEventListener("resize", describe);
  new MutationObserver(describe).observe(document.body, {
    attributes: true,
    attributeFilter: ["data-dev-side", "data-devtools"],
  });
  describe();
}
