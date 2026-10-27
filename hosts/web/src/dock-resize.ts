/** The least the developer tools can shrink to. The stylesheet keeps 120px of app above them. */
const MIN_HEIGHT = 160;
const KEY_STEP = 24;

/**
 * Let the top edge of the developer tools, docked at the bottom, be dragged to set their height.
 *
 * The height is one CSS variable on the tools, so nothing is moved or
 * re-created and the product frame keeps running. The stylesheet clamps the
 * result to the space the window has.
 */
export function bindDockResize(handle: HTMLElement, dock: HTMLElement): void {
  const maxHeight = (): number =>
    Math.max(MIN_HEIGHT, window.innerHeight - 120);
  const set = (height: number): void => {
    const clamped = Math.round(
      Math.min(maxHeight(), Math.max(MIN_HEIGHT, height)),
    );
    dock.style.setProperty("--devtools-h", `${clamped}px`);
    handle.setAttribute("aria-valuenow", String(clamped));
  };
  let dragging = false;

  handle.addEventListener("pointerdown", (event) => {
    dragging = true;
    handle.setPointerCapture(event.pointerId);
    document.body.style.userSelect = "none";
    event.preventDefault();
  });
  handle.addEventListener("pointermove", (event) => {
    if (dragging) set(window.innerHeight - event.clientY);
  });
  const end = (): void => {
    dragging = false;
    document.body.style.userSelect = "";
  };
  handle.addEventListener("pointerup", end);
  handle.addEventListener("pointercancel", end);
  handle.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    const step = event.key === "ArrowUp" ? KEY_STEP : -KEY_STEP;
    set(dock.getBoundingClientRect().height + step);
  });
}
