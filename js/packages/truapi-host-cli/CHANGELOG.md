<!--
Copyright 2026 Parity Technologies (UK) Ltd.
SPDX-License-Identifier: MIT
-->

# Changelog

All notable changes to `@dotli/host-cli`. This package versions
independently of the dotli app (see README, "Versioning and releases").

## 0.1.0 (unreleased)

Initial release: a terminal host for `@parity/truapi-host` 0.23.0 (wire
codec 3).

- The full required typed callback surface plus the optional
  permission-status probe, bridged through the package's own generated
  adapter (no hand-written SCALE). The optional `chat`, `contacts` and
  `pocket` groups are deliberately absent, since a terminal host holds no
  chat lists, contacts or cards.
- `productOperations`: pending-operation bookkeeping with ids unique among
  one product's open operations and idempotent ends. In a browser these
  references keep a product's worker alive; this host runs the core
  in-process, so they are recorded rather than acted on.
- RFC-0026 `supportedChains` advertisement, role-mapped from the endpoint
  map so it can never disagree with `featureSupported` or `chain.connect`.
- Locale subscription (BCP 47), defaulting to the process locale.
- Confirm prompts for every review the core can raise, including the
  RFC-0023 `SignVrf`, the requesting product on every signing review that
  names one, the `StatementStoreProductSign` statement payload
  (signed as-is, never presented with the raw-message convention) and the
  `ProductSubtree` account-key resolution.
- In-process wasm boot (`initSync`) and an in-process loopback wire for
  same-process products.
- Terminal presenter: pairing QR (offline, instant), progress through the
  silent `Authenticating` window, deliberately modest confirm prompts that
  defer content verification to the paired wallet, auto-deny on non-TTY.
- Prompt routing for embedded contexts: `input: "tty"` asks on the
  controlling terminal when the standard streams belong to a parent process
  (git remote helpers), and denies when no terminal exists.
- Batch context on prompts: when confirms queue up (bulk Bulletin writes),
  each prompt states how many approvals wait behind it.
- Bulletin publish prompts state that terminal approval is FINAL: those
  writes are signed in-core with the login-time allowance and never reach
  the phone, so the phone-checkpoint line was wrong in the unsafe direction
  (verified with the wallet offline: phone-bound operations time out while
  Bulletin writes succeed).
- Owner-only (0600) JSON file storage for core and product state. Product
  storage cleared on logout and on identity change.
- Chain-connection pool keyed by genesis hash: per-lease request-id
  rewriting, subscription routing, capped leases per socket,
  order-preserving delivery.
- `serializeOperationStarts`: the load-bearing product-side shim for the
  chain-head operation-ordering hazard.
- `explainProductError` / `isProbableSsoTimeout`: translate the untyped 180s
  SSO timeout into actionable guidance.
