//! Inter-host SSO with the paired wallet: `pairing` bootstraps the
//! QR/deeplink handshake. The session-channel payloads (`messages`) and the
//! request/response typing (`wire`) are crate-internal and live in
//! `crate::host_internal`.

pub mod pairing;
