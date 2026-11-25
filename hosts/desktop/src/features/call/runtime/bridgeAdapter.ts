// Wraps a MessagePort (transferred from the main process via Electron's
// MessageChannelMain) into a typed send/receive interface for the call bridge.

import { type MainToWindowMessage, type WindowToMainMessage, parseMainToWindow } from '@/domains/chat';

/**
 * Creates a bridge adapter over a MessagePort.
 * Inbound: validates each message with parseMainToWindow; drops invalid frames.
 * Outbound: posts WindowToMainMessage directly — the main renderer validates
 * with parseWindowToMain on its side.
 */
export function createBridgeAdapter(
  port: MessagePort,
  onMessage: (m: MainToWindowMessage) => void,
): { send: (m: WindowToMainMessage) => void } {
  port.onmessage = (event: MessageEvent) => {
    const msg = parseMainToWindow(event.data);
    if (msg !== null) {
      onMessage(msg);
    }
  };

  port.start();

  return {
    send: (m: WindowToMainMessage) => port.postMessage(m),
  };
}
