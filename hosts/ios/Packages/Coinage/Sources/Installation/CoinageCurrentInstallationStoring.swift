import Foundation

/// The one row naming the current installation, kept in the same database as the coins and vouchers
/// allocated in it. Implemented in the app over CoreData.
public protocol CoinageCurrentInstallationRepositoryProtocol: Sendable {
    /// The current installation, created with `newInstallation()` when the row does not exist. The
    /// read and the insert must share one transaction: two concurrent first callers get the same id,
    /// or one device's keys would be split across two subtrees.
    func getOrCreateCurrent(
        newInstallation: @escaping @Sendable () throws -> CoinageInstallationId
    ) async throws -> CoinageInstallationId
}

/// The Keychain tags an installation's allocation counters are stored under. The app supplies the
/// layout; the tags are scoped by the installation itself, so a counter can never serve another one.
public protocol CoinageInstallationKeychainTagProviding: Sendable {
    func coinIndexTag(for installation: CoinageInstallationId) -> String
    func voucherIndexTag(for installation: CoinageInstallationId) -> String
}

/// The one installation new keys are allocated in, and the counters that hand out its items.
///
/// Items are never re-issued: each `next…Item` call reserves an item for good, whether or not the
/// coin or voucher built for it is ever saved, so a crash between the two costs one unused key.
public protocol CoinageCurrentInstallationStoring: Sendable {
    /// The current installation, created on first call.
    func getOrCreateCurrent() async throws -> CoinageInstallationId

    func nextCoinItem() async throws -> DerivationIndex

    func nextVoucherItem() async throws -> DerivationIndex
}
