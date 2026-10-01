import { Layout, type Panel } from "./layout.js";

/** Below this width the panels cover the product instead of sitting beside it. */
const NARROW = "(max-width: 900px)";

export interface ChromeElements {
  appbar: HTMLElement;
  stage: HTMLElement;
  scrim: HTMLElement;
  panels: Record<Panel, HTMLElement>;
  toggles: Record<Panel, HTMLButtonElement>;
}

export interface Chrome {
  /** Close the panel covering the product, if one does. */
  dismissCovering(): void;
}

/**
 * Show and hide the menu and the inspector around the product.
 *
 * Panels only change classes and attributes on the page around the product
 * frame. Nothing here moves or re-creates the frame's element, so opening or
 * closing a panel never reloads the product or touches the session.
 */
export function bindChrome(elements: ChromeElements): Chrome {
  const { appbar, stage, scrim, panels, toggles } = elements;
  const narrow = window.matchMedia(NARROW);
  const layout = new Layout(narrow.matches ? [] : ["menu"]);
  layout.setModal(narrow.matches);
  let covering: Panel | null = null;

  const render = (): void => {
    const before = covering;
    covering = layout.covering;
    for (const panel of ["menu", "inspector"] as const) {
      const open = layout.isOpen(panel);
      document.body.dataset[panel] = open ? "open" : "closed";
      toggles[panel].setAttribute("aria-expanded", String(open));
    }
    // A covering panel is modal: what it covers cannot take focus or clicks.
    scrim.hidden = covering === null;
    stage.inert = appbar.inert = covering !== null;

    if (covering !== null && covering !== before) panels[covering].focus();
    if (covering === null && before !== null) {
      const focus = document.activeElement;
      if (focus === document.body || panels[before].contains(focus))
        toggles[before].focus();
    }
  };

  for (const panel of ["menu", "inspector"] as const) {
    toggles[panel].addEventListener("click", () => {
      layout.toggle(panel);
      render();
    });
    panels[panel]
      .querySelector(`[data-close="${panel}"]`)
      ?.addEventListener("click", () => {
        layout.close(panel);
        render();
      });
  }

  scrim.addEventListener("click", () => {
    layout.dismissCovering();
    render();
  });

  // A consent prompt is a dialog of its own and handles Escape itself.
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || document.querySelector("dialog[open]"))
      return;
    if (layout.dismissCovering()) {
      event.preventDefault();
      render();
    }
  });

  narrow.addEventListener("change", () => {
    layout.setModal(narrow.matches);
    render();
  });

  render();
  return {
    dismissCovering() {
      layout.dismissCovering();
      render();
    },
  };
}
