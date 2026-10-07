//! Wire discriminants the library addresses.
//!
//! Hardcoded, because the generated wire table lives in `truapi`'s runtime
//! build, which a wasm32 product does not link. `wasm-worker-probe` pins every
//! one of them to the generated table, so a renumbered method fails there
//! rather than on a live host.

/// `system.handshake`.
pub const SYSTEM_HANDSHAKE: (u8, u8) = (1, 0);
/// `chat.createRoom`.
pub const CHAT_CREATE_ROOM: (u8, u8) = (4, 0);
/// `chat.registerBot`.
pub const CHAT_REGISTER_BOT: (u8, u8) = (4, 1);
/// `chat.postMessage`.
pub const CHAT_POST_MESSAGE: (u8, u8) = (4, 3);
/// `renderer.render`, host initiated.
pub const RENDERER_RENDER: (u8, u8) = (17, 0);
/// `renderer.actionSubscribe`.
pub const RENDERER_ACTION_SUBSCRIBE: (u8, u8) = (17, 1);

/// Request leg of a call, and the start leg of a subscription.
pub const MESSAGE_TYPE_REQUEST: u8 = 0;
/// Response leg of a call, and an item of a subscription.
pub const MESSAGE_TYPE_RESPONSE: u8 = 1;
/// Terminal leg of a subscription.
pub const MESSAGE_TYPE_INTERRUPT: u8 = 2;
/// The subscriber's cancellation of a subscription.
pub const MESSAGE_TYPE_STOP: u8 = 3;
