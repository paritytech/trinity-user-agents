import { createState } from '@/shared/rxstate';

export const inputSurfaceOpen = createState(false);

/**
 * What the field holds when the surface opens — the address bar hands over
 * whatever it was showing.
 *
 * Written by the same handler that opens the surface, and read once at mount, so
 * it can never describe an opening other than the current one.
 */
export const inputSurfaceInitialText = createState('');
