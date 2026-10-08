//! Unified TrUAPI trait set.

pub mod account;
pub mod chain;
pub mod chat;
pub mod coin_payment;
pub mod contacts;
pub mod entropy;
pub mod game;
pub mod local_storage;
pub mod locale;
pub mod notifications;
pub mod payment;
pub mod permissions;
pub mod pocket;
pub mod preimage;
pub mod renderer;
pub mod resource_allocation;
pub mod scanner;
pub mod signing;
pub mod statement_store;
pub mod system;
pub mod theme;
pub mod worker;

pub use account::Account;
pub use chain::Chain;
pub use chat::Chat;
pub use coin_payment::CoinPayment;
pub use contacts::Contacts;
pub use entropy::Entropy;
pub use game::Game;
pub use local_storage::LocalStorage;
pub use locale::Locale;
pub use notifications::Notifications;
pub use payment::Payment;
pub use permissions::Permissions;
pub use pocket::Pocket;
pub use preimage::Preimage;
pub use renderer::Renderer;
pub use resource_allocation::ResourceAllocation;
pub use scanner::Scanner;
pub use signing::Signing;
pub use statement_store::StatementStore;
pub use system::System;
pub use theme::Theme;
pub use worker::Worker;

/// The unified TrUAPI contract.
pub trait TrUApi:
    Account
    + Chain
    + Chat
    + CoinPayment
    + Contacts
    + Entropy
    + Game
    + LocalStorage
    + Locale
    + Notifications
    + Payment
    + Permissions
    + Pocket
    + Preimage
    + Renderer
    + ResourceAllocation
    + Scanner
    + Signing
    + StatementStore
    + System
    + Theme
    + Worker
    + Send
    + Sync
{
}

impl<T> TrUApi for T where
    T: Account
        + Chain
        + Chat
        + CoinPayment
        + Contacts
        + Entropy
        + Game
        + LocalStorage
        + Locale
        + Notifications
        + Payment
        + Permissions
        + Pocket
        + Preimage
        + Renderer
        + ResourceAllocation
        + Scanner
        + Signing
        + StatementStore
        + System
        + Theme
        + Worker
        + Send
        + Sync
{
}
