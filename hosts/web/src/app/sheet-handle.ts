/**
 * The grab handle a sheet carries, made to do what it shows. A tap on the
 * handle closes the sheet. A drag down from the handle, or from the sheet's
 * header, moves the sheet with the finger and closes it when let go far or
 * fast enough; a shorter drag springs back. The handle never resizes the
 * sheet, and a drag up does nothing.
 *
 * A drag starts only on the handle or the header, never on a control in the
 * header, and only once it moves down more than sideways, so buttons, tabs
 * and the sheet's own scrolling keep their gestures. The handle is a pointer
 * target only: keyboard and assistive users close the sheet as before, by
 * its close button or Escape.
 */

/** How far a pointer moves before it counts as a drag, in CSS pixels. */
const DRAG_START = 8;
/** A release faster than this, in CSS pixels per millisecond, is a flick. */
const FLICK_SPEED = 0.5;
/** A flick must still have moved the sheet this far. */
const FLICK_DISTANCE = 24;
/** A slow release closes at this distance, or a quarter of the sheet when that is less. */
const CLOSE_DISTANCE = 120;
const SPRING_BACK = "transform 200ms cubic-bezier(0.16, 1, 0.3, 1)";

/** What a drag may not start on: the header's own controls. */
const CONTROLS =
  "button, a[href], input, select, textarea, summary, label, [role='tab'], [role='separator'], [contenteditable]";

export interface SheetHandleElements {
  /** The handle: a tap on it closes the sheet. It acts only while it is drawn. */
  handle: HTMLElement;
  /** Where else a drag down may start, such as the sheet's header. */
  header?: HTMLElement[];
  /** What follows the finger while it drags. */
  sheet: HTMLElement;
  /** Close the sheet as its close button or Escape does. */
  close(): void;
}

/**
 * Whether a drag let go after moving the sheet `distance` pixels down, at
 * `speed` pixels per millisecond, closes a sheet `height` pixels tall.
 */
export function releaseCloses(
  distance: number,
  speed: number,
  height: number,
): boolean {
  if (distance <= 0) return false;
  if (distance >= Math.min(CLOSE_DISTANCE, height / 4)) return true;
  return speed >= FLICK_SPEED && distance >= FLICK_DISTANCE;
}

/** Make `handle` close the sheet on a tap, and it or the header on a drag down. */
export function bindSheetHandle(elements: SheetHandleElements): void {
  const { handle, sheet, close } = elements;
  const areas = [handle, ...(elements.header ?? [])];
  const drawn = () => handle.getClientRects().length > 0;
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  let drag: {
    pointer: number;
    area: HTMLElement;
    startX: number;
    startY: number;
    moving: boolean;
    distance: number;
    /** The sheet's drawn height over its layout height, as a scaled prompt has. */
    scale: number;
    samples: { y: number; time: number }[];
  } | null = null;
  // A drag that ends on the handle is followed by a click, which must not close the sheet again.
  let dragged = false;

  const place = (distance: number, scale: number) => {
    sheet.style.transform =
      distance > 0 ? `translateY(${distance / scale}px)` : "";
  };

  const end = (closes: boolean) => {
    if (drag === null) return;
    const { area, pointer, moving } = drag;
    drag = null;
    if (area.hasPointerCapture(pointer)) area.releasePointerCapture(pointer);
    if (!moving) return;
    dragged = true;
    if (closes) {
      sheet.style.transition = "";
      sheet.style.transform = "";
      close();
      return;
    }
    sheet.style.transition = reducedMotion.matches ? "" : SPRING_BACK;
    sheet.style.transform = "";
    setTimeout(() => {
      if (drag === null) sheet.style.transition = "";
    }, 300);
  };

  for (const area of areas) {
    area.style.touchAction = "none";
    area.addEventListener("pointerdown", (event) => {
      dragged = false;
      if (drag !== null || !event.isPrimary || event.button !== 0) return;
      if (!drawn()) return;
      const target = event.target as Element;
      const control = target.closest(CONTROLS);
      if (control !== null && area.contains(control)) return;
      const rect = sheet.getBoundingClientRect();
      drag = {
        pointer: event.pointerId,
        area,
        startX: event.clientX,
        startY: event.clientY,
        moving: false,
        distance: 0,
        scale: sheet.offsetHeight > 0 ? rect.height / sheet.offsetHeight : 1,
        samples: [{ y: event.clientY, time: event.timeStamp }],
      };
      area.setPointerCapture(event.pointerId);
    });
    area.addEventListener("pointermove", (event) => {
      if (drag === null || event.pointerId !== drag.pointer) return;
      const across = event.clientX - drag.startX;
      const down = event.clientY - drag.startY;
      if (!drag.moving) {
        if (Math.abs(across) > DRAG_START && Math.abs(across) >= down)
          return end(false);
        if (down < -DRAG_START) return end(false);
        if (down <= DRAG_START) return;
        drag.moving = true;
        sheet.style.transition = "none";
      }
      drag.distance = Math.max(0, down);
      drag.samples = [
        ...drag.samples.filter((sample) => event.timeStamp - sample.time < 100),
        { y: event.clientY, time: event.timeStamp },
      ];
      place(drag.distance, drag.scale);
    });
    area.addEventListener("pointerup", (event) => {
      if (drag === null || event.pointerId !== drag.pointer) return;
      if (drag.moving) drag.distance = Math.max(0, event.clientY - drag.startY);
      const first = drag.samples[0]!;
      const elapsed = event.timeStamp - first.time;
      const speed = elapsed > 0 ? (event.clientY - first.y) / elapsed : 0;
      end(
        releaseCloses(
          drag.distance,
          speed,
          sheet.getBoundingClientRect().height,
        ),
      );
    });
    area.addEventListener("pointercancel", (event) => {
      if (drag !== null && event.pointerId === drag.pointer) end(false);
    });
  }

  handle.addEventListener("click", () => {
    if (dragged) {
      dragged = false;
      return;
    }
    if (drawn()) close();
  });
}
