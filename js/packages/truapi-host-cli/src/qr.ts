// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import * as QRCode from "qrcode";

/**
 * Render a pairing deeplink as a scannable terminal QR (ANSI half-blocks).
 * The deeplink is produced by the core with no network at all, so this can be
 * drawn instantly and offline.
 */
export function renderQrTerminal(text: string): Promise<string> {
  return QRCode.toString(text, { type: "terminal", small: true });
}
