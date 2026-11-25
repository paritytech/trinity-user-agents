/**
 * Entrance stagger for a row in the surface, shared by the suggestions and the
 * candidate cards so a merged list animates as one thing rather than two lists
 * that happen to agree.
 *
 * Only the first handful stagger. Answers arrive on their own schedule anyway,
 * so past the cap the real arrival times already do the work and a growing delay
 * would just make a late row feel broken.
 */
const STAGGER_STEP_MS = 22;
const STAGGER_CAP = 6;

/** The delay a row at `index` waits before its entrance. */
export function staggerDelay(index: number): string {
  return `${Math.min(index, STAGGER_CAP) * STAGGER_STEP_MS}ms`;
}
