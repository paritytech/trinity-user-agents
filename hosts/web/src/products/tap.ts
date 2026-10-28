/** Which way a frame crossed, from the product's side. */
export type FrameDirection = "in" | "out";

/** Observes one product's frames as the page relays them. */
export type FrameTap = (direction: FrameDirection, frame: Uint8Array) => void;
