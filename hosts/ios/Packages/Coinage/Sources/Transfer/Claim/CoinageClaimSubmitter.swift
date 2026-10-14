import Foundation
import KeyDerivation
import ExtrinsicService
import SDKLogger

/// A coin a peer handed us, ready to claim: its keypair and the on-chain value exponent to mint into.
/// What the chain says about a coin a peer handed us, read before we claim it.
struct ClaimableCoinInfo: Equatable {
    let exponent: Int16
    /// On-chain age as it stands now. Our claim transfer will add one to it.
    let age: Int16
}

struct ClaimableCoin {
    let privateKey: Data
    let publicKey: Data
    let valueExponent: Int16
    /// On-chain age before our claim, so the coin's provenance can be reconstructed at mint time.
    let age: Int16
}

/// Claims coins a peer handed us: each is transferred into a fresh address of ours, registered through
/// the durability layer under one `groupId` so the whole claim group is recorded or none of it is.
///
/// The peer's key is a `Received` input — never a local asset — so the ledger holds it against exactly
/// one non-failure claim without us ever minting it.
///
/// One registration per coin, for the life of the message. Retries belong to the submission policy,
/// which rebuilds into the destination this claim recorded — a retry minting a fresh destination would
/// strand any payment already registered against the first.
///
/// The store *would* permit a fresh registration once an entry reaches `FAILURE` (`filterClaimed`
/// excludes failed entries for exactly that reason), but nothing issues one: the claim loop counts a
/// failed entry as a coin already registered. So a coin whose policy has given up is not recoverable
/// in-app, and its secret stays in the chat row.
protocol CoinageClaimSubmitting: Sendable {
    /// Registers one claim per coin and returns once registration commits — not settlement. Status is
    /// observed via ``CoinageTxServicing/subscribeOperationGroupStatuses(_:)`` for `groupId`.
    ///
    /// `bundleSize` is how many coins the transfer moved in total, which is not the same as
    /// `claimable.count`: claiming retries, so one pass may see only part of the group.
    func submit(
        claimable: [ClaimableCoin],
        bundleSize: Int,
        groupId: CoinageTxGroupId,
        retryUntil: Date
    ) async throws
}

final class CoinageClaimSubmitter: CoinageClaimSubmitting, @unchecked Sendable {
    private let minter: any CoinMinting
    private let originFactory: any OriginCreating
    private let txService: any CoinageTxServicing
    private let logger: SDKLoggerProtocol?

    init(
        minter: any CoinMinting,
        originFactory: any OriginCreating,
        txService: any CoinageTxServicing,
        logger: SDKLoggerProtocol?
    ) {
        self.minter = minter
        self.originFactory = originFactory
        self.txService = txService
        self.logger = logger
    }

    func submit(
        claimable: [ClaimableCoin],
        bundleSize: Int,
        groupId: CoinageTxGroupId,
        retryUntil: Date
    ) async throws {
        guard !claimable.isEmpty else { return }

        // Each claim is signed by a different peer key, so these are distinct origins built
        // independently and registered together — atomically under one groupId.
        var requests: [CoinageTxRequest] = []
        for coin in claimable {
            // Every coin in the transfer shares its bundle size, including ones a later pass claims.
            try await requests.append(
                buildClaim(coin, bundleSize: bundleSize, groupId: groupId, retryUntil: retryUntil)
            )
        }

        logger?.debug("Registering \(requests.count) claim(s) for group \(groupId)")
        try await txService.submitTransactions(requests, groupId: groupId)
    }
}

// MARK: - Private

private extension CoinageClaimSubmitter {
    /// Mints a fresh destination coin (persisted on allocation, so registration can link the output
    /// row) and builds a transfer of the received coin into it, signed by the peer's key.
    func buildClaim(
        _ coin: ClaimableCoin,
        bundleSize: Int,
        groupId: CoinageTxGroupId,
        retryUntil: Date
    ) async throws -> CoinageTxRequest {
        // Nothing in a peer's coin reveals the recycler it came out of, but the chain does give its
        // age — enough to reconstruct a conservative chain of one transfer per unit of it, with this
        // claim as the most recent hop. The transfer we are about to submit ages it by one.
        let destination = try await minter.mintCoin(
            exponent: coin.valueExponent,
            provenance: .received(ageAfterTransfer: coin.age + 1, bundleSize: bundleSize)
        )

        let coinPrivateKey = coin.privateKey
        let wallet = DynamicDerivedWallet(secretKeyProvider: { coinPrivateKey })
        let origin = try originFactory.createAsCoinOrigin(for: wallet)

        let call = CoinagePallet.Calls.Transfer(to: destination.publicKey)
        let builder: ExtrinsicBuilderClosure = { try $0.adding(call: call.callAsFunction()) }

        logger?.debug("Built claim group=\(groupId) value=\(coin.valueExponent)")

        return try CoinageTxRequest(
            inputs: [.coin(.received(coin.publicKey))],
            outputs: [.coin(destination.derivationIndex, destination.publicKey)],
            builder: builder,
            origin: origin,
            // A failed claim is rebuilt into *this* destination coin: a claim retried into a fresh one
            // would strand any payment already registered against the first.
            policy: CoinageSubmissionParams.claimPolicy(
                ClaimSubmissionParams(retryUntil: retryUntil, receivedKey: coinPrivateKey)
            )
        )
    }
}
