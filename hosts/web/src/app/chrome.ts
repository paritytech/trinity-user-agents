import { DEV_VIEWS, Layout, type DevSide, type DevView } from "./layout.js";

/** Below this width the tools cover the app instead of sitting beside it. */
const NARROW = "(max-width: 900px)";
const SIDE_KEY = "truapi-web-signing-host:devtools-side";

export interface ChromeElements {
  stage: HTMLElement;
  scrim: HTMLElement;
  /** The developer tools panel. */
  panel: HTMLElement;
  /** The Focus capsule: shows or hides the tools at the view they last showed. */
  focus: HTMLButtonElement;
  close: HTMLButtonElement;
}

export interface Chrome {
  /** Close the tools if they cover the app. */
  dismissCovering(): void;
  /** Open the tools at `view`. */
  show(view: DevView): void;
}

const loadSide = (): DevSide =>
  localStorage.getItem(SIDE_KEY) === "bottom" ? "bottom" : "right";

/**
 * Show and hide the developer tools beside the app, at the right or the
 * bottom, and switch their view. Closed, they leave the app the whole window:
 * Focus. On a narrow screen they cover the app instead.
 *
 * Only classes and attributes on the page around the product frame change.
 * Nothing here moves or re-creates the frame's element, so opening, closing
 * or moving the tools never reloads the product or touches the session.
 */
export function bindChrome(elements: ChromeElements): Chrome {
  const { stage, scrim, panel, focus } = elements;
  const narrow = window.matchMedia(NARROW);
  const layout = new Layout(!narrow.matches);
  layout.setModal(narrow.matches);
  let side = loadSide();
  let covering = false;
  let wasOpen = layout.open;
  const tabs = new Map(
    DEV_VIEWS.map((view) => [
      view,
      panel.querySelector<HTMLButtonElement>(`[data-dev-tab="${view}"]`)!,
    ]),
  );
  const views = new Map(
    DEV_VIEWS.map((view) => [
      view,
      panel.querySelector<HTMLElement>(`[data-dev-panel="${view}"]`)!,
    ]),
  );
  const sides = [
    ...panel.querySelectorAll<HTMLButtonElement>("button[data-dev-side]"),
  ];

  const render = (): void => {
    const before = covering;
    covering = layout.covering;
    const body = document.body.dataset;
    body.devtools = layout.open ? "open" : "closed";
    body.devView = layout.view;
    body.devSide = side;
    body.inspector = layout.isShown("inspector") ? "open" : "closed";
    body.menu = layout.open && layout.view !== "inspector" ? "open" : "closed";
    focus.setAttribute("aria-expanded", String(layout.open));
    focus.title = layout.open
      ? "Hide the developer tools (Focus)"
      : "Show the developer tools";
    for (const [view, tab] of tabs) {
      tab.setAttribute("aria-selected", String(layout.view === view));
      tab.tabIndex = layout.view === view ? 0 : -1;
      views.get(view)!.hidden = layout.view !== view;
    }
    for (const button of sides)
      button.setAttribute(
        "aria-pressed",
        String(button.dataset.devSide === side),
      );
    // Open tools that cover the app are modal: what they cover takes no input.
    scrim.hidden = !covering;
    stage.inert = covering;

    if (covering && !before) panel.focus();
    if (
      wasOpen &&
      !layout.open &&
      (panel.contains(document.activeElement) ||
        document.activeElement === document.body)
    )
      focus.focus();
    wasOpen = layout.open;
  };

  focus.addEventListener("click", () => {
    layout.toggleOpen();
    render();
  });
  for (const [view, tab] of tabs)
    tab.addEventListener("click", () => {
      layout.show(view);
      render();
    });
  panel
    .querySelector('[role="tablist"]')
    ?.addEventListener("keydown", (event) => {
      const key = (event as KeyboardEvent).key;
      const step = key === "ArrowRight" ? 1 : key === "ArrowLeft" ? -1 : null;
      if (step === null) return;
      event.preventDefault();
      const index = DEV_VIEWS.indexOf(layout.view);
      const next =
        DEV_VIEWS[(index + step + DEV_VIEWS.length) % DEV_VIEWS.length];
      layout.show(next);
      render();
      tabs.get(next)!.focus();
    });
  for (const button of sides)
    button.addEventListener("click", () => {
      side = button.dataset.devSide === "bottom" ? "bottom" : "right";
      localStorage.setItem(SIDE_KEY, side);
      render();
    });
  elements.close.addEventListener("click", () => {
    layout.close();
    render();
  });

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
    show(view) {
      layout.show(view);
      render();
    },
  };
}
