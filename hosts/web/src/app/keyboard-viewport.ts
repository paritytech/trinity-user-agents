/** What a browser says of the area the user can see, which an on-screen keyboard shrinks. */
export interface VisibleArea {
  height: number;
  offsetTop: number;
  scale: number;
}

/**
 * The page's area while an on-screen keyboard covers its bottom, or null when
 * nothing does.
 *
 * Some phone browsers (iOS Safari among them) leave the layout viewport at
 * full height when the keyboard opens and only shrink the visible area, so a
 * page that fills `100dvh` keeps its bottom, and with it the Docklet and the
 * search field, under the keyboard. Browsers that resize the layout already
 * report no difference, so no inset is added twice. A pinch zoom also shrinks
 * the visible area, and is not a keyboard.
 */
export function keyboardArea(
  visible: VisibleArea,
  layoutHeight: number,
): { height: number; top: number } | null {
  if (Math.abs(visible.scale - 1) > 0.01) return null;
  if (layoutHeight - visible.height - visible.offsetTop < 1) return null;
  return { height: visible.height, top: visible.offsetTop };
}

/**
 * Keep the app inside what the user can see: set `--app-height` and
 * `--app-top` on `root` while a keyboard covers part of the page, and remove
 * them when it does not. Returns a function that stops following.
 */
export function followKeyboard(
  root: HTMLElement,
  visual: (EventTarget & VisibleArea) | null,
): () => void {
  if (visual === null) return () => {};
  const apply = (): void => {
    const area = keyboardArea(visual, root.clientHeight);
    if (area === null) {
      root.style.removeProperty("--app-height");
      root.style.removeProperty("--app-top");
      return;
    }
    root.style.setProperty("--app-height", `${area.height}px`);
    root.style.setProperty("--app-top", `${area.top}px`);
  };
  visual.addEventListener("resize", apply);
  visual.addEventListener("scroll", apply);
  apply();
  return () => {
    visual.removeEventListener("resize", apply);
    visual.removeEventListener("scroll", apply);
    root.style.removeProperty("--app-height");
    root.style.removeProperty("--app-top");
  };
}
