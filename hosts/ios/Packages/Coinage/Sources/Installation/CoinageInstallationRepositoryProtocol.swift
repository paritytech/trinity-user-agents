import Foundation

/// A coinage derivation subtree this account is known to own but did not allocate on this device: it
/// is only ever scanned for balance, and the scan progress belongs to it.
public struct PreviousInstallation: Hashable, Sendable {
    public let id: CoinageInstallationId
    public let coinScanNextIndex: DerivationIndex
    public let voucherScanNextIndex: DerivationIndex
    public let initialScanCompleted: Bool
    /// The user has accepted whatever the scans found here; false again only for an installation
    /// recorded later, since a new one is worth showing.
    public let isUserConfirmedCompletion: Bool

    /// Whether the balance found here is still to be shown to the user.
    public var awaitsUserConfirmation: Bool {
        initialScanCompleted && !isUserConfirmedCompletion
    }

    public init(
        id: CoinageInstallationId,
        coinScanNextIndex: DerivationIndex,
        voucherScanNextIndex: DerivationIndex,
        initialScanCompleted: Bool,
        isUserConfirmedCompletion: Bool
    ) {
        self.id = id
        self.coinScanNextIndex = coinScanNextIndex
        self.voucherScanNextIndex = voucherScanNextIndex
        self.initialScanCompleted = initialScanCompleted
        self.isUserConfirmedCompletion = isUserConfirmedCompletion
    }
}

/// The previous installations the data store lists for this account. The current one has a row of its
/// own (``CoinageCurrentInstallationRepositoryProtocol``) and is never recorded here. Implemented in the
/// app over CoreData.
public protocol CoinageInstallationRepositoryProtocol: Sendable {
    /// Records installations as previous ones — unscanned and unconfirmed — skipping any already known.
    /// Callers exclude the current installation themselves.
    func addPrevious(_ installations: [CoinageInstallationId]) async throws

    func getPrevious() async throws -> [PreviousInstallation]

    func updateCoinScanNextIndex(_ nextIndex: DerivationIndex, for installation: CoinageInstallationId) async throws

    func updateVoucherScanNextIndex(_ nextIndex: DerivationIndex, for installation: CoinageInstallationId) async throws

    func markInitialScanCompleted(_ installation: CoinageInstallationId) async throws

    /// The user accepted the recovered balance: every installation known now is confirmed.
    func markAllUserConfirmed() async throws
}
