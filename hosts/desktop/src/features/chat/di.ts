import { createAnyOf, createSlot } from '@/shared/di';
import { type ChatSession } from '@/domains/chat';

/** Injected into the Room header's action row — left of the search button.
 *  Props: the current ChatSession, so callers can read peer name / send messages. */
export const chatRoomHeaderActionsSlot = createSlot<{ session: ChatSession }>({ name: 'chatRoomHeaderActionsSlot' });

/** Full-width banner strip at the very top of the Room conversation view (above
 *  the header) — e.g. the return-to-call bar. Scoped to the open chat, not the list. */
export const chatRoomBannerSlot = createSlot<{ session: ChatSession }>({ name: 'chatRoomBannerSlot' });

/** Capability probe: can the user place a call from inside the app right now?
 *  The call bubble reads it to choose between "Tap to …" and "Open Mobile App to …" copy. */
export const canPlaceCallAnyOf = createAnyOf({ name: 'canPlaceCall' });
