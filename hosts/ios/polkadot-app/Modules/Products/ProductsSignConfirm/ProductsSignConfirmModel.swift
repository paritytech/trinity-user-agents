import Foundation

/// Wallet-free view model for the rust-core signing confirmation sheet. Built
/// directly from the review — no account/wallet resolution — because the rust
/// core performs the signing itself after the user approves.
struct ProductsSignConfirmModel {
    let requester: PolkadotSigningRequester
    let descriptionText: String
    let detailsText: String
    let isTransaction: Bool
}
