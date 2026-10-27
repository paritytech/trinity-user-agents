/** A size the product rectangle can take. `null` sizes fill the stage. */
interface Viewport {
  id: string;
  label: string;
  width: number | null;
  height: number | null;
}

const FILL: Viewport = { id: "fill", label: "Fill", width: null, height: null };
const PHONE: Viewport = {
  id: "phone",
  label: "Phone 393 × 852",
  width: 393,
  height: 852,
};
const VIEWPORTS: Viewport[] = [
  FILL,
  PHONE,
  { id: "tablet", label: "Tablet 820 × 1180", width: 820, height: 1180 },
  { id: "desktop", label: "Desktop 1280 × 800", width: 1280, height: 800 },
];

/** Fill when the stage is at least this wide, else Phone. */
export const AUTO_MIN_WIDTH = 760;
const AUTO = "auto";
/** The entry after the presets: a size the user sets by dragging or typing. */
const CUSTOM = "custom";
const CUSTOM_LABEL = "Custom";

/** What surrounds a rectangle with a set size. */
export const FRAMES = ["device", "round", "none"] as const;
export type FrameKind = (typeof FRAMES)[number];
/** The width of the device frame's bezel, in CSS pixels. */
const BEZEL = 12;
/** Room kept around a rectangle with a set size, in CSS pixels. */
const MARGIN = 24;
/** Room under it for its caption. */
const CAPTION = 30;

const STORAGE_KEY = "truapi-web-signing-host:tab-viewport";
const CUSTOM_STORAGE_KEY = "truapi-web-signing-host:tab-viewport-custom";
const FRAME_STORAGE_KEY = "truapi-web-signing-host:tab-viewport-frame";

/** The least and most a custom side may be, in CSS pixels. */
export const MIN_SIDE = 200;
export const MAX_SIDE = 4000;
const DEFAULT_CUSTOM: Size = { width: 480, height: 720 };
const KEY_STEP = 10;

export interface Size {
  width: number;
  height: number;
}

/** A custom side, rounded and held between the bounds. Text that is not a number gives `fallback`. */
export function clampSide(value: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(MAX_SIDE, Math.max(MIN_SIDE, Math.round(value)));
}

/**
 * The size a drag gives. The pointer moves in screen pixels and the frame is
 * drawn at `factor` of its size, so a screen delta is divided by the factor to
 * become CSS pixels. The result stays within the bounds and, where given,
 * within `limit`, the most the stage can show at that factor.
 */
export function draggedSize(
  start: Size,
  delta: { x: number; y: number },
  factor: number,
  limit?: Size,
): Size {
  const side = (from: number, move: number, cap: number | undefined) => {
    const wanted = from + move / factor;
    return clampSide(cap === undefined ? wanted : Math.min(wanted, cap), from);
  };
  return {
    width: side(start.width, delta.x, limit?.width),
    height: side(start.height, delta.y, limit?.height),
  };
}

/** The custom size saved for this tab. Anything unreadable gives the default. */
export function loadCustomSize(storage: Pick<Storage, "getItem">): Size {
  try {
    const saved: unknown = JSON.parse(
      storage.getItem(CUSTOM_STORAGE_KEY) ?? "null",
    );
    if (typeof saved !== "object" || saved === null) return DEFAULT_CUSTOM;
    const { width, height } = saved as Record<string, unknown>;
    return {
      width: clampSide(Number(width), DEFAULT_CUSTOM.width),
      height: clampSide(Number(height), DEFAULT_CUSTOM.height),
    };
  } catch {
    return DEFAULT_CUSTOM;
  }
}

export interface ViewportElements {
  select: HTMLSelectElement;
  /** Shows the scale when the rectangle is shrunk to fit. */
  scale: HTMLElement;
  area: HTMLElement;
  frame: HTMLElement;
  /** True on a screen too small to simulate another one: the product fills it. */
  fills: MediaQueryList;
  /** The width and height fields, shown only for the custom size. */
  custom: {
    group: HTMLElement;
    width: HTMLInputElement;
    height: HTMLInputElement;
  };
  /** The corner handle that resizes a custom rectangle by dragging. */
  handle: HTMLElement;
  /** What the app gets and how it is drawn, in the developer tools. */
  note: HTMLElement;
  /** The same, short, under a rectangle with a set size. */
  caption: HTMLElement;
  /** Buttons that choose a size by id, such as Auto, Fill and Phone. */
  presets: HTMLButtonElement[];
  /** Buttons that choose the frame around a set size, by kind. */
  frames: HTMLButtonElement[];
  /** Holds the frame buttons, shown only while the size is set. */
  frameGroup: HTMLElement;
}

/**
 * Give the product frame a chosen CSS size, shrunk to fit the stage.
 *
 * Only the product rectangle changes. The window and the developer tools
 * keep their own size, and the frame element is resized in place, so changing
 * the size never reloads the product. The shrink is a transform applied after
 * layout, so the product lays out at the chosen size rather than the scaled
 * one. On a screen the query in `fills` matches, the product fills the stage
 * and the choice is ignored.
 *
 * The custom size is anchored at the stage's top-left so that its corner
 * handle sits under the pointer. A drag is measured against the scale the
 * rectangle has when the drag starts and holds it until release, so the corner
 * follows the pointer; the fields set an exact size, up to the bounds.
 */
const bold = (text: string): HTMLElement => {
  const node = document.createElement("b");
  node.textContent = text;
  return node;
};

/** The developer tools' note: the app's size in bold, then the window and the tools. */
function noteLines(size: string, how: string): (Node | string)[] {
  const tools = document.getElementById("devtools");
  const zone =
    tools === null || tools.offsetWidth === 0
      ? "dev tools hidden"
      : `dev tools ${tools.offsetWidth} × ${tools.offsetHeight}`;
  return [
    bold(size),
    ` · ${how}`,
    document.createElement("br"),
    `Window ${window.innerWidth} × ${window.innerHeight} · ${zone}`,
  ];
}

export function bindViewport(elements: ViewportElements): void {
  const { select, scale, area, frame, fills, custom, handle } = elements;
  select.replaceChildren(
    new Option(`Auto: Fill from ${AUTO_MIN_WIDTH} px, else Phone`, AUTO),
    ...VIEWPORTS.map((viewport) => new Option(viewport.label, viewport.id)),
    new Option(CUSTOM_LABEL, CUSTOM),
  );
  select.value = FILL.id;
  const saved = sessionStorage.getItem(STORAGE_KEY);
  if (saved !== null && [...select.options].some((o) => o.value === saved))
    select.value = saved;
  const savedFrame = sessionStorage.getItem(FRAME_STORAGE_KEY);
  let frameKind: FrameKind =
    FRAMES.find((kind) => kind === savedFrame) ?? "device";

  let customSize = loadCustomSize(sessionStorage);
  /** The scale held for the length of a drag, or null when none is under way. */
  let dragFactor: number | null = null;

  const isCustom = () => select.value === CUSTOM;
  /** The size the choice gives now: Auto follows the stage's width. */
  const chosen = (): Viewport =>
    select.value === AUTO
      ? area.clientWidth >= AUTO_MIN_WIDTH
        ? FILL
        : PHONE
      : (VIEWPORTS.find((viewport) => viewport.id === select.value) ?? FILL);

  const renderButtons = (sized: boolean) => {
    for (const button of elements.presets)
      button.setAttribute(
        "aria-pressed",
        String(button.dataset.viewport === select.value),
      );
    for (const button of elements.frames)
      button.setAttribute(
        "aria-pressed",
        String(button.dataset.frame === frameKind),
      );
    elements.frameGroup.hidden = !sized;
  };

  /** Where the handle sits: the bottom-right corner of the rectangle as drawn. */
  const placeHandle = () => {
    const box = frame.getBoundingClientRect();
    const stage = area.getBoundingClientRect();
    handle.style.left = `${box.right - stage.left - handle.offsetWidth}px`;
    handle.style.top = `${box.bottom - stage.top - handle.offsetHeight}px`;
  };

  const fit = () => {
    const resizable = isCustom() && !fills.matches;
    const viewport = isCustom()
      ? { ...customSize, id: CUSTOM, label: CUSTOM_LABEL }
      : chosen();
    custom.group.hidden = !resizable;
    handle.hidden = !resizable;
    area.dataset.custom = String(resizable);
    frame.style.transformOrigin = resizable ? "top left" : "";
    if (fills.matches || viewport.width === null || viewport.height === null) {
      frame.style.width = frame.style.height = "100%";
      frame.style.transform = "";
      frame.classList.remove("sized");
      delete frame.dataset.frame;
      delete frame.dataset.shape;
      scale.textContent = "";
      renderButtons(false);
      elements.caption.hidden = true;
      elements.note.replaceChildren(
        ...(fills.matches
          ? []
          : noteLines(
              `App viewport ${area.clientWidth} × ${area.clientHeight}`,
              "fills the app zone",
            )),
      );
      return;
    }
    renderButtons(true);
    frame.classList.add("sized");
    frame.dataset.frame = frameKind;
    frame.dataset.shape = viewport.width < 600 ? "phone" : "tablet";
    frame.style.width = `${viewport.width}px`;
    frame.style.height = `${viewport.height}px`;
    const bezel = 2 * MARGIN + (frameKind === "device" ? 2 * BEZEL : 0);
    const factor =
      dragFactor ??
      Math.min(
        1,
        (area.clientWidth - bezel) / viewport.width,
        (area.clientHeight - bezel - CAPTION) / viewport.height,
      );
    frame.style.transform = factor === 1 ? "" : `scale(${factor})`;
    const percent = `${Math.round(factor * 100)}%`;
    scale.textContent = factor === 1 ? "" : percent;
    elements.note.replaceChildren(
      ...noteLines(
        `App viewport ${viewport.width} × ${viewport.height}`,
        `drawn at ${percent}`,
      ),
    );
    if (resizable) elements.caption.hidden = true;
    else {
      // Under the rectangle as drawn: the transform does not move layout.
      elements.caption.hidden = false;
      const box = frame.getBoundingClientRect();
      const zone = area.getBoundingClientRect();
      const name = viewport.label.split(" ")[0];
      elements.caption.replaceChildren(
        bold(`${name} ${viewport.width} × ${viewport.height}`),
        ` ${percent} · simulated`,
      );
      elements.caption.style.top = `${box.bottom - zone.top + (frameKind === "device" ? BEZEL : 0) + 8}px`;
    }
    if (resizable) {
      if (document.activeElement !== custom.width)
        custom.width.value = String(viewport.width);
      if (document.activeElement !== custom.height)
        custom.height.value = String(viewport.height);
      placeHandle();
    }
  };

  const setCustom = (next: Size) => {
    customSize = next;
    sessionStorage.setItem(CUSTOM_STORAGE_KEY, JSON.stringify(next));
    fit();
  };

  select.addEventListener("change", () => {
    sessionStorage.setItem(STORAGE_KEY, select.value);
    fit();
  });
  for (const button of elements.presets)
    button.addEventListener("click", () => {
      select.value = button.dataset.viewport!;
      select.dispatchEvent(new Event("change"));
    });
  for (const button of elements.frames)
    button.addEventListener("click", () => {
      frameKind =
        FRAMES.find((kind) => kind === button.dataset.frame) ?? "device";
      sessionStorage.setItem(FRAME_STORAGE_KEY, frameKind);
      fit();
    });
  fills.addEventListener("change", fit);

  // A typed side takes effect on each valid change, and is put back in range
  // when the field is left.
  for (const [field, side] of [
    [custom.width, "width"],
    [custom.height, "height"],
  ] as const) {
    field.min = String(MIN_SIDE);
    field.max = String(MAX_SIDE);
    field.addEventListener("input", () => {
      const value = Number(field.value);
      if (field.value !== "" && value >= MIN_SIDE && value <= MAX_SIDE)
        setCustom({ ...customSize, [side]: Math.round(value) });
    });
    field.addEventListener("change", () => {
      field.value = String(clampSide(Number(field.value), customSize[side]));
      setCustom({
        ...customSize,
        [side]: clampSide(Number(field.value), customSize[side]),
      });
    });
  }

  let drag: { x: number; y: number; start: Size; limit: Size } | null = null;
  handle.addEventListener("pointerdown", (event) => {
    const box = frame.getBoundingClientRect();
    const factor = box.width / customSize.width;
    dragFactor = factor;
    drag = {
      x: event.clientX,
      y: event.clientY,
      start: customSize,
      limit: {
        width: area.clientWidth / factor,
        height: area.clientHeight / factor,
      },
    };
    handle.setPointerCapture(event.pointerId);
    document.body.style.userSelect = "none";
    event.preventDefault();
  });
  handle.addEventListener("pointermove", (event) => {
    if (drag === null || dragFactor === null) return;
    setCustom(
      draggedSize(
        drag.start,
        { x: event.clientX - drag.x, y: event.clientY - drag.y },
        dragFactor,
        drag.limit,
      ),
    );
  });
  const endDrag = () => {
    if (drag === null) return;
    drag = null;
    dragFactor = null;
    document.body.style.userSelect = "";
    fit();
  };
  handle.addEventListener("pointerup", endDrag);
  handle.addEventListener("pointercancel", endDrag);
  handle.addEventListener("keydown", (event) => {
    const arrows: Record<string, [number, number]> = {
      ArrowRight: [KEY_STEP, 0],
      ArrowLeft: [-KEY_STEP, 0],
      ArrowDown: [0, KEY_STEP],
      ArrowUp: [0, -KEY_STEP],
    };
    const move = arrows[event.key];
    if (move === undefined) return;
    event.preventDefault();
    setCustom({
      width: clampSide(customSize.width + move[0], customSize.width),
      height: clampSide(customSize.height + move[1], customSize.height),
    });
  });

  new ResizeObserver(fit).observe(area);
  fit();
}
