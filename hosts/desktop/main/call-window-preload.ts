// This preload runs in the call-window's renderer process (isolated context).
// It forwards the MessagePort transferred from the main process into the
// call-window's main world via window.postMessage + transferable ports.

import { ipcRenderer } from 'electron';

// The main process sends 'call:init' with the call launch params as the data
// argument and the MessagePort as a transferable in event.ports.
// We relay both into the main world so main.tsx can pick them up via
// window.addEventListener('message', ...).
ipcRenderer.on('call:init', (event, data: unknown) => {
  // Target our own origin (not '*') so the transferred port is never delivered
  // to an unexpected origin; main.tsx additionally checks event.source.
  window.postMessage({ __callInit: data }, window.location.origin, event.ports);
});
