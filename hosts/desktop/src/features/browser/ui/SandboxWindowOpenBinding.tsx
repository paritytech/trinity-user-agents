import { useSandboxWindowOpenRouting } from '../hooks/useSandboxWindowOpenRouting';

// Headless: answers main's `window.open` routing question for the app lifetime.
// Rendered once via persistentSlot; renders nothing.
export const SandboxWindowOpenBinding = () => {
  useSandboxWindowOpenRouting();
  return null;
};
