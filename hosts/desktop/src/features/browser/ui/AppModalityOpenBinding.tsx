import { useAnnounceAppOpen } from '../hooks/useAnnounceAppOpen';

// Headless: drives the app-open announcement for the app lifetime. Rendered once
// via persistentSlot; renders nothing.
export const AppModalityOpenBinding = () => {
  useAnnounceAppOpen();
  return null;
};
