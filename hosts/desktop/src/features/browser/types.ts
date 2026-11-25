export type SwipeDirection = 'back' | 'forward';

// Structurally equal to the Webview widget's `onGuestWheel` payload.
export type SwipeWheel = { deltaX: number; deltaY: number; consumed: boolean };

export type SwipeState =
  | { status: 'idle' }
  | { status: 'deciding'; deltaX: number; deltaY: number }
  | { status: 'rejected' }
  | { status: 'tracking'; direction: SwipeDirection; distance: number; lastMagnitude: number; decays: number }
  | { status: 'committed'; direction: SwipeDirection };

export type SwipeIndicatorState = { direction: SwipeDirection; progress: number };

export type SwipeAvailability = { canGoBack: boolean; canGoForward: boolean };
