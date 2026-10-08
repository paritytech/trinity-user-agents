//! Unified TrUAPI trait set.
//!
//! Each trait's wire id, set by `#[wire_trait(id = N)]`, is permanent once
//! released. Codegen rejects two traits with one id in a tree; this table
//! also holds ids taken by work not merged yet, so a collision shows in
//! review instead of after both land. Claim the next free id here.
//!
//! | Id | Trait |
//! |----|-------|
//! | 1 | `System` |
//! | 2 | `Account` |
//! | 3 | `Chain` |
//! | 4 | `Chat` |
//! | 5 | `CoinPayment` |
//! | 6 | `Entropy` |
//! | 7 | `LocalStorage` |
//! | 8 | `Notifications` |
//! | 9 | `Payment` |
//! | 10 | `Permissions` |
//! | 11 | `Preimage` |
//! | 12 | `ResourceAllocation` |
//! | 13 | `Signing` |
//! | 14 | `StatementStore` |
//! | 15 | `Theme` |
//! | 16 | `Locale` |
//! | 17 | `Renderer` |
//! | 18 | `Pocket` |
//! | 19 | `Worker` |
//! | 20 | `Contacts` |
//! | 21 | `Game` |
//! | 22 | `Funding` |
//! | 23 | reserved: expanded card face |
//! | 24 | `FundingProvider` |
//! | 25 | `Scanner` |

pub mod account;
pub mod chain;
pub mod chat;
pub mod coin_payment;
pub mod contacts;
pub mod entropy;
pub mod funding;
pub mod funding_provider;
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
pub use funding::Funding;
pub use funding_provider::FundingProvider;
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
    + Funding
    + Game
    + FundingProvider
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
        + Funding
        + Game
        + FundingProvider
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
