// The steps fundtest reports, by name, and how each goes on the wire.

import type {
  FundingDirection,
  FundingInboundUpdate,
  FundingOutboundUpdate,
  FundingUpdate,
} from "@parity/truapi";

/** A step of a session as fundtest names it: a step of its direction, or
 *  one every session shares. */
export type Step =
  | FundingInboundUpdate
  | FundingOutboundUpdate
  | Exclude<FundingUpdate, { tag: "In" } | { tag: "Out" }>;

/** `step` as the core takes it, grouped by the session's `direction`. */
export function onWire(step: Step, direction: FundingDirection): FundingUpdate {
  switch (step.tag) {
    case "Failed":
    case "Details":
    case "Progress":
      return step;
    default:
      return direction === "In"
        ? { tag: "In", value: step as FundingInboundUpdate }
        : { tag: "Out", value: step as FundingOutboundUpdate };
  }
}

/** The step `update` carries, whichever group it came in. */
export function step(update: FundingUpdate): Step {
  return update.tag === "In" || update.tag === "Out" ? update.value : update;
}
