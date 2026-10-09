---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the host-owned `Media` API for real-time sessions, authenticated participant invitations, incoming-offer decisions, local microphone/camera/screen intent, and host-composited surfaces. Sessions belong to the authenticated product, account, network, and runtime. Calling and capture consent remain separate, screen capture uses the trusted picker, and operation identifiers support cancellation and recovery without duplicate effects. Capability discovery reports unsupported hosts without requesting permission or starting capture.

Media TypeScript examples consume observable session snapshots and retain their host listener through mutation admission and cleanup. The mock host preserves product, root-account, and Bulletin-genesis isolation for automatic-preimage consent and quota storage. Android storage fixtures exercise scoped identity and isolation, and Media validation helpers preserve commit, capture, and surface checks without duplicated throw paths.
