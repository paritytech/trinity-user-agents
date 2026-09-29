/** A viewport the product can be shown at. `null` sizes fill the stage. */
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

/**
 * Give the product frame a device's CSS viewport, scaled down to fit the
 * stage. The scale is a transform, applied after layout, so the product lays
 * out at the device's size rather than the scaled one.
 */
export function bindViewport(
  select: HTMLSelectElement,
  area: HTMLElement,
  frame: HTMLElement,
): void {
  select.replaceChildren(
    ...VIEWPORTS.map(
      (viewport, index) => new Option(viewport.label, String(index)),
    ),
  );

  const fit = () => {
    const viewport = VIEWPORTS[Number(select.value)] ?? VIEWPORTS[0];
    if (viewport.width === null || viewport.height === null) {
      frame.style.width = frame.style.height = "100%";
      frame.style.transform = "";
      return;
    }
    frame.style.width = `${viewport.width}px`;
    frame.style.height = `${viewport.height}px`;
    const scale = Math.min(
      1,
      area.clientWidth / viewport.width,
      area.clientHeight / viewport.height,
    );
    frame.style.transform = scale === 1 ? "" : `scale(${scale})`;
  };

  select.addEventListener("change", fit);
  new ResizeObserver(fit).observe(area);
  fit();
}
