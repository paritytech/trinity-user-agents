/** A size the product rectangle can take. `null` sizes fill the stage. */
interface Viewport {
  label: string;
  width: number | null;
  height: number | null;
}

const VIEWPORTS: Viewport[] = [
  { label: "Fill", width: null, height: null },
  { label: "Phone 393 × 852", width: 393, height: 852 },
  { label: "Tablet 820 × 1180", width: 820, height: 1180 },
  { label: "Desktop 1280 × 800", width: 1280, height: 800 },
];

const STORAGE_KEY = "truapi-web-signing-host:tab-viewport";

export interface ViewportElements {
  select: HTMLSelectElement;
  /** Shows the scale when the rectangle is shrunk to fit. */
  scale: HTMLElement;
  area: HTMLElement;
  frame: HTMLElement;
  /** Receives `data-dock`: `side` beside a portrait rectangle, else `bottom`. */
  root: HTMLElement;
  /** True on a screen too small to simulate another one: the product fills it. */
  fills: MediaQueryList;
}

/**
 * Give the product frame a chosen CSS size, shrunk to fit the stage.
 *
 * Only the product rectangle changes. The host's menu, top bar and inspector
 * keep their own size, and the frame element is resized in place, so changing
 * the size never reloads the product. The shrink is a transform applied after
 * layout, so the product lays out at the chosen size rather than the scaled
 * one. On a screen the query in `fills` matches, the product fills the stage
 * and the choice is ignored.
 */
export function bindViewport(elements: ViewportElements): void {
  const { select, scale, area, frame, root, fills } = elements;
  select.replaceChildren(
    ...VIEWPORTS.map(
      (viewport, index) => new Option(viewport.label, String(index)),
    ),
  );
  const saved = sessionStorage.getItem(STORAGE_KEY);
  if (saved !== null && VIEWPORTS[Number(saved)]) select.value = saved;

  const fit = () => {
    const viewport = VIEWPORTS[Number(select.value)] ?? VIEWPORTS[0];
    if (fills.matches || viewport.width === null || viewport.height === null) {
      frame.style.width = frame.style.height = "100%";
      frame.style.transform = "";
      frame.classList.remove("sized");
      scale.textContent = "";
      root.dataset.dock = "bottom";
      return;
    }
    frame.classList.add("sized");
    root.dataset.dock = viewport.height > viewport.width ? "side" : "bottom";
    frame.style.width = `${viewport.width}px`;
    frame.style.height = `${viewport.height}px`;
    const factor = Math.min(
      1,
      area.clientWidth / viewport.width,
      area.clientHeight / viewport.height,
    );
    frame.style.transform = factor === 1 ? "" : `scale(${factor})`;
    scale.textContent = factor === 1 ? "" : `${Math.round(factor * 100)}%`;
  };

  select.addEventListener("change", () => {
    sessionStorage.setItem(STORAGE_KEY, select.value);
    fit();
  });
  fills.addEventListener("change", fit);
  new ResizeObserver(fit).observe(area);
  fit();
}
