import { type SwipeState } from './types';

export const SWIPE_THRESHOLD_PX = 200;
// Accumulated |deltaX| + |deltaY| before the gesture's axis is decided; a single tick is too small to tell axis from drift.
export const SWIPE_AXIS_SLOP_PX = 4;
// Electron reports no gesture phase, so a gesture ends when wheel input pauses this long.
export const SWIPE_GESTURE_END_MS = 150;
// Consecutive same-direction, non-increasing |deltaX| events that mark macOS momentum, i.e. the fingers have lifted.
export const SWIPE_LIFT_DECAYS = 3;
export const SWIPE_IDLE: SwipeState = { status: 'idle' };
