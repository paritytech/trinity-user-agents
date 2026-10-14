import DurableTransactions
import Foundation
import SDKLogger
import SubstrateSdk

/// Reads one batch of a previous installation's subtree from chain: the coins and vouchers that exist
/// there, keyed to the indices they were derived from.
protocol InstallationAssetScanning: Sendable {
    func scanCoins(
        installation: CoinageInstallationId,
        startIndex: DerivationIndex,
        count: DerivationIndex
    ) async throws -> [Coin]

    func scanVouchers(
        installation: CoinageInstallationId,
        startIndex: DerivationIndex,
        count: DerivationIndex
    ) async throws -> [Voucher]
}

/// Reads at the finalized head: a recovered row is treated as finalized-minted on this read alone.
final class InstallationAssetScanner: InstallationAssetScanning, @unchecked Sendable {
    private let coinKeypairFactory: any CoinKeyDeriving
    private let coinOnChainQuery: any CoinOnChainQuerying
    private let voucherOnChainQuery: any VoucherOnChainQuerying
    private let chainViewFactory: any PinnedChainViewFactoryProtocol
    private let chainId: ChainId
    private let logger: (any SDKLoggerProtocol)?

    init(
        coinKeypairFactory: any CoinKeyDeriving,
        coinOnChainQuery: any CoinOnChainQuerying,
        voucherOnChainQuery: any VoucherOnChainQuerying,
        chainViewFactory: any PinnedChainViewFactoryProtocol,
        chainId: ChainId,
        logger: (any SDKLoggerProtocol)?
    ) {
        self.coinKeypairFactory = coinKeypairFactory
        self.coinOnChainQuery = coinOnChainQuery
        self.voucherOnChainQuery = voucherOnChainQuery
        self.chainViewFactory = chainViewFactory
        self.chainId = chainId
        self.logger = logger
    }

    func scanCoins(
        installation: CoinageInstallationId,
        startIndex: DerivationIndex,
        count: DerivationIndex
    ) async throws -> [Coin] {
        let indexedKeys = try Self.indices(of: installation, from: startIndex, count: count).map {
            try (index: $0, publicKey: coinKeypairFactory.derivePublicKey(index: $0))
        }
        guard !indexedKeys.isEmpty else { return [] }

        let finalizedHash = try await finalizedHead()
        let onChain = try await coinOnChainQuery.fetchCoins(
            for: indexedKeys.map(\.publicKey),
            atBlockHash: finalizedHash
        )

        return zip(indexedKeys, onChain).compactMap { key, info -> Coin? in
            guard let info else { return nil }
            // Recovered from a read that found it, so it is on chain by construction. Recovery reads value
            // and age, never where the coin has been, so its provenance stays unknown.
            return Coin(
                exponent: Int16(info.value),
                derivationIndex: key.index,
                age: info.age,
                isOnchain: true,
                publicKey: key.publicKey
            )
        }
    }

    func scanVouchers(
        installation: CoinageInstallationId,
        startIndex: DerivationIndex,
        count: DerivationIndex
    ) async throws -> [Voucher] {
        let indices = Self.indices(of: installation, from: startIndex, count: count)
        guard !indices.isEmpty else { return [] }

        let finalizedHash = try await finalizedHead()
        let onChain = try await voucherOnChainQuery.fetchVouchers(for: indices, atBlockHash: finalizedHash)

        return zip(indices, onChain).compactMap { index, info -> Voucher? in
            guard let info, let state = Self.recoverableState(of: info) else { return nil }
            // Recovery knows where the voucher sits, not how drained its ring is: fungibility stays zero
            // and the ceiling unfrozen until the location service reads a real one.
            return Voucher(
                exponent: info.exponent,
                derivationIndex: index,
                allocatedAt: .now,
                readyAt: .distantPast,
                remoteState: state,
                publicKey: info.publicKey
            )
        }
    }
}

private extension InstallationAssetScanner {
    func finalizedHead() async throws -> Data {
        try await chainViewFactory.pin(chainId: chainId).finalizedHead.hash
    }

    static func indices(
        of installation: CoinageInstallationId,
        from startIndex: DerivationIndex,
        count: DerivationIndex
    ) -> [CoinageKeyIndex] {
        (startIndex ..< startIndex + count).map { CoinageKeyIndex(installation: installation, item: $0) }
    }

    /// An unloaded voucher is spent, and a suspended one has no ring to be released from — neither is
    /// balance worth recovering. Onboarding and ring-placed vouchers are.
    static func recoverableState(of info: VoucherOnChainInfo) -> Voucher.OnChainState? {
        guard !info.isUnloaded else { return nil }
        if info.ringPosition.isOnboarding { return .onboarding }
        guard info.ringPosition.ringIndex != nil else { return nil }
        return info.onChainState
    }
}
