import AsyncExtensions
import BigInt
import Foundation
import NovaCrypto
import SDKLogger
import SubstrateSdk
import SubstrateSdkExt
import SubstrateOperation

/// Detects coins for raw secret keys and recovers locally-spent coins. The chat claim path now lives
/// in ``ClaimCoinsService`` (durability-driven); this service delegates to it for the actual
/// claim and keeps the on-chain detection and spent-coin recovery that have no other home.
public protocol TransferClaimServicing: Actor {
    /// Detects on-chain coins for the given sr25519 secret keys; when
    /// `transferCoins` is true, also claims them into the user's coin set.
    /// Returns the total detected value in planks (0 if none found).
    func transferCoinsFromSecretKeys(
        secretKeys: [Data],
        transferCoins: Bool,
        context: DenominationBreakdownContext
    ) async throws -> BigUInt

    /// Recovers locally-spent coins that may still be on-chain:
    /// - Coins whose on-chain age has reached `coinMaxAge` are flipped back to
    ///   `.available` locally — the chain would reject a transfer for them, so we
    ///   leave them in place and rely on the recycling service to sweep them next.
    /// - Younger coins are revoked by transferring them to fresh destinations.
    /// Returns the total planks recovered across both paths.
    func recoverSpentCoins(
        spentCoins: [Coin],
        context: DenominationBreakdownContext
    ) async throws -> BigUInt
}

/// Outcome of a send confirmation that tolerates a recipient claim.
public enum SendConfirmation: Sendable {
    /// At least one coin is still on-chain — send confirmed, awaiting claim.
    case onChain
    /// All coins are confirmed sent then spent — the recipient already claimed.
    case alreadyClaimed
}

/// Waits until outgoing transfer coins have appeared on-chain, or throws on timeout.
public protocol TransferSendVerifying: Actor {
    func awaitSendOnChain(memo: TransferMemo, blockTimeout: UInt32) async throws
    func awaitClaimOnChain(memo: TransferMemo, blockTimeout: UInt32) async throws

    /// Confirms the transfer reached the chain even if the recipient already claimed some or all
    /// coins. `anchorBlock` (e.g. submit-time finalized block + send window) enables a historical
    /// probe for coins spent before this watch began. Throws `CoinOnChainQueryError.timeout` only
    /// when a coin is neither present now, nor seen during the watch, nor present at the anchor —
    /// i.e. a genuine lost send.
    func awaitSendOrClaimed(
        memo: TransferMemo,
        anchorBlock: BlockNumber?,
        blockTimeout: UInt32
    ) async throws -> SendConfirmation
}

public extension TransferSendVerifying {
    /// Default: fall back to strict presence detection (no claim tolerance).
    func awaitSendOrClaimed(
        memo: TransferMemo,
        anchorBlock _: BlockNumber?,
        blockTimeout: UInt32
    ) async throws -> SendConfirmation {
        try await awaitSendOnChain(memo: memo, blockTimeout: blockTimeout)
        return .onChain
    }
}

/// Combined protocol for services that handle both claiming and send verification.
public typealias OngoingTransferServicing = TransferClaimServicing & TransferSendVerifying

public enum TransferRecipientError: Error {
    case coinNotFound
}

actor TransferRecipientService {
    typealias OnChainCoin = CoinSyncResult.OnChainCoin
    private typealias HeadsSharedStream = AsyncShareSequence<AnyAsyncSequence<Block.Header>>

    private let coinMinter: any CoinMinting
    private let coinKeyFactory: any CoinKeyDeriving
    private let coinService: any CoinServiceProtocol
    private let coinOnChainQuery: any CoinOnChainQuerying
    private let transferSubmitter: any CoinTransferSubmitting
    private let snKeyFactory: any SNKeyFactoryProtocol
    private let claimCoinsService: any ClaimCoinsServicing
    private let blockNumberProvider: any BlockInfoProviding
    private let logger: SDKLoggerProtocol?

    /// Shared finalized-head multicast.
    /// Started on first `awaitSendOnChain` caller, released on last.
    private var headsSharedStream: HeadsSharedStream?
    private var headsWaiterCount = 0

    init(
        coinMinter: any CoinMinting,
        coinKeyFactory: any CoinKeyDeriving,
        coinService: any CoinServiceProtocol,
        coinOnChainQuery: any CoinOnChainQuerying,
        transferSubmitter: any CoinTransferSubmitting,
        snKeyFactory: any SNKeyFactoryProtocol,
        claimCoinsService: any ClaimCoinsServicing,
        blockNumberProvider: any BlockInfoProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.coinMinter = coinMinter
        self.coinKeyFactory = coinKeyFactory
        self.coinService = coinService
        self.coinOnChainQuery = coinOnChainQuery
        self.transferSubmitter = transferSubmitter
        self.snKeyFactory = snKeyFactory
        self.claimCoinsService = claimCoinsService
        self.blockNumberProvider = blockNumberProvider
        self.logger = logger
    }
}

// MARK: - Cancellable async sequence wrapper

private extension AsyncSequence where Element: Sendable, AsyncIterator: Sendable {
    /// Bridges a non-cancellation-aware async sequence (e.g. an AsyncExtensions multicast
    /// subject) into an AsyncStream that finishes promptly when the consuming task is
    /// cancelled, instead of staying suspended until the next upstream element.
    func cancellable() -> AsyncStream<Element> {
        AsyncStream { continuation in
            let task = Task {
                do {
                    for try await element in self {
                        continuation.yield(element)
                    }
                } catch {}
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}

// MARK: - TransferClaimServicing

extension TransferRecipientService: OngoingTransferServicing {
    func transferCoinsFromSecretKeys(
        secretKeys: [Data],
        transferCoins: Bool,
        context: DenominationBreakdownContext
    ) async throws -> BigUInt {
        guard !secretKeys.isEmpty else { return 0 }

        let publicKeys: [PublicKey] = try secretKeys.map {
            try snKeyFactory.createPublicKey(fromSecret: $0).rawData()
        }
        let onChainCoins = try await coinOnChainQuery.fetchCoins(for: publicKeys)

        var total: BigUInt = 0
        for coin in onChainCoins {
            guard let coin else { continue }
            total += context.valueInPlanks(for: Int16(coin.value))
        }

        guard transferCoins, total > 0 else { return total }

        // Content-addressed group so a retry after process death rejoins the claims already
        // registered rather than submitting a second set.
        let groupId = try "w3s-coins-\(TransferMemo(entries: secretKeys, totalValue: total).identifier().toHex())"
        let retryUntil = Date().addingTimeInterval(CoinageConstants.secretKeyClaimTimeout)

        for try await _ in claimCoinsService.claim(
            coinKeys: secretKeys,
            groupId: groupId,
            retryUntil: retryUntil,
            context: context
        ) {}

        return total
    }

    func recoverSpentCoins(
        spentCoins: [Coin],
        context: DenominationBreakdownContext
    ) async throws -> BigUInt {
        guard !spentCoins.isEmpty else { return .zero }

        let senderKeys: [SenderKey] = try spentCoins.enumerated().map { index, coin in
            let privateKey = try coinKeyFactory.derivePrivateKey(for: coin)
            let publicKey = try snKeyFactory.createPublicKey(fromSecret: privateKey).rawData()
            return (index, privateKey, publicKey)
        }

        let sourceCoins = try await coinOnChainQuery.fetchCoins(for: senderKeys.map(\.publicKey))

        let restoredPlanks = try await restoreMaxAgeCoins(
            spentCoins: spentCoins,
            sourceCoins: sourceCoins,
            context: context
        )

        let (transferableKeys, transferableSources) = filterTransferable(
            senderKeys: senderKeys,
            sourceCoins: sourceCoins
        )

        var failures: [EntryFailure] = []
        let (prepared, _) = await allocateDestinations(
            senderKeys: transferableKeys,
            sourceCoins: transferableSources,
            failures: &failures
        )

        guard !prepared.isEmpty else { return restoredPlanks }

        let (claimed, submitFailures) = await submitTransfers(prepared)
        failures.append(contentsOf: submitFailures)

        if !claimed.isEmpty {
            try await coinService.save(coins: claimed)
            logger?.debug("Saved \(claimed.count) revoked coins")
        }

        if !failures.isEmpty {
            logger?.error("Revoke had \(failures.count) failures")
        }

        let transferredPlanks = claimed.reduce(.zero) { $0 + context.valueInPlanks(for: $1.exponent) }
        return restoredPlanks + transferredPlanks
    }
}

// MARK: - Send verification

extension TransferRecipientService {
    /// Derives sender public keys, then races a finalized-block counter against the coin subscription.
    ///
    /// All concurrent callers share one `subscribeFinalizedHeads()` WebSocket subscription
    /// via `headsShare`. Throws `CoinOnChainQueryError.timeout` once `blockTimeout` finalized
    /// heads have been observed without all coins appearing on-chain.
    func awaitSendOnChain(memo: TransferMemo, blockTimeout: UInt32) async throws {
        guard !memo.entries.isEmpty else { return }

        let publicKeys: [PublicKey] = try memo.entries.map {
            try snKeyFactory.createPublicKey(fromSecret: $0).rawData()
        }

        let stream = acquireHeadStream()
        defer { releaseHeadStream() }

        let memoId = try memo.identifier().toHexString()
        logger?.debug("\(memoId) start, timeout \(blockTimeout) blocks")

        try await withThrowingTaskGroup(of: Void.self) { group in
            group.addTask { [logger] in
                var count: UInt32 = 0
                for try await header in stream.cancellable() {
                    count += 1
                    if count == 1 {
                        let memoId = try memo.identifier().toHexString()
                        logger?.debug(
                            "\(memoId) started from block \(header.number.hexBlockNumber)"
                        )
                    }
                    if count >= blockTimeout {
                        throw CoinOnChainQueryError.timeout
                    }
                }
                throw CoinOnChainQueryError.subscriptionTerminated
            }

            group.addTask { [self] in
                try await coinOnChainQuery.awaitAllCoinsOnChain(for: publicKeys)
            }

            try await group.next()
            group.cancelAll()
        }
    }

    /// Waits until transferred coins have been claimed and are no longer on-chain.
    ///
    /// Derives destination public keys from memo entries (opposite of awaitSendOnChain),
    /// then races a finalized-block counter against the coin subscription. Completes when
    /// all coin keys are absent/spent on-chain (recipient has claimed them), or throws
    /// `CoinOnChainQueryError.timeout` after `blockTimeout` blocks.
    func awaitClaimOnChain(memo: TransferMemo, blockTimeout: UInt32) async throws {
        guard !memo.entries.isEmpty else { return }

        let publicKeys: [PublicKey] = try memo.entries.map {
            try snKeyFactory.createPublicKey(fromSecret: $0).rawData()
        }

        let stream = acquireHeadStream()
        defer { releaseHeadStream() }

        let memoId = try memo.identifier().toHexString()
        logger?.debug("\(memoId) start, timeout \(blockTimeout) blocks")

        try await withThrowingTaskGroup(of: Void.self) { [logger] group in
            group.addTask {
                var count: UInt32 = 0
                for try await header in stream.cancellable() {
                    count += 1
                    if count == 1 {
                        let memoId = try memo.identifier().toHexString()
                        logger?.debug(
                            "\(memoId) started from block \(header.number.hexBlockNumber)"
                        )
                    }
                    if count >= blockTimeout {
                        throw CoinOnChainQueryError.timeout
                    }
                }
                throw CoinOnChainQueryError.subscriptionTerminated
            }

            group.addTask { [self] in
                try await coinOnChainQuery.awaitAllCoinsOffChain(for: publicKeys)
            }

            try await group.next()
            group.cancelAll()
        }
    }

    func awaitSendOrClaimed(
        memo: TransferMemo,
        anchorBlock: BlockNumber?,
        blockTimeout: UInt32
    ) async throws -> SendConfirmation {
        guard !memo.entries.isEmpty else { return .onChain }

        let publicKeys: [PublicKey] = try memo.entries.map {
            try snKeyFactory.createPublicKey(fromSecret: $0).rawData()
        }

        var confirmed = Set<Int>()
        var anyPresentNow = false

        // 1. Current on-chain state.
        let current = try await coinOnChainQuery.fetchCoins(for: publicKeys, atBlockHash: nil)
        for (index, coin) in current.enumerated() where coin != nil {
            confirmed.insert(index)
            anyPresentNow = true
        }

        // 2. Historical probe: a coin spent before this watch is absent now but was present at the
        //    anchor block. Best-effort — a future/unavailable anchor simply skips this step.
        if confirmed.count < publicKeys.count, let anchorBlock {
            let absentIndices = (0 ..< publicKeys.count).filter { !confirmed.contains($0) }
            if let anchorHash = try? await blockNumberProvider.fetchBlockHash(anchorBlock) {
                let absentKeys = absentIndices.map { publicKeys[$0] }
                let historical = try await coinOnChainQuery.fetchCoins(
                    for: absentKeys,
                    atBlockHash: anchorHash
                )
                for (offset, coin) in historical.enumerated() where coin != nil {
                    confirmed.insert(absentIndices[offset])
                }
            }
        }

        if confirmed.count == publicKeys.count {
            return anyPresentNow ? .onChain : .alreadyClaimed
        }

        // 3. Live subscription for the remaining keys: confirm via present-or-spent, racing timeout.
        let remainingKeys = (0 ..< publicKeys.count)
            .filter { !confirmed.contains($0) }
            .map { publicKeys[$0] }

        let anyRemainingPresent = try await raceCoinsSentOrClaimed(
            for: remainingKeys,
            blockTimeout: blockTimeout
        )

        return (anyPresentNow || anyRemainingPresent) ? .onChain : .alreadyClaimed
    }

    private func raceCoinsSentOrClaimed(for publicKeys: [Data], blockTimeout: UInt32) async throws -> Bool {
        let stream = acquireHeadStream()
        defer { releaseHeadStream() }

        logger?.debug("start, timeout \(blockTimeout) blocks")

        return try await withThrowingTaskGroup(of: Bool.self) { group in
            group.addTask { [logger] in
                var count: UInt32 = 0
                for try await header in stream.cancellable() {
                    count += 1
                    if count == 1 {
                        logger?.debug(
                            "started from block \(header.number.hexBlockNumber)"
                        )
                    }
                    if count >= blockTimeout {
                        throw CoinOnChainQueryError.timeout
                    }
                }
                throw CoinOnChainQueryError.subscriptionTerminated
            }

            group.addTask { [self] in
                try await coinOnChainQuery.awaitAllCoinsSentOrClaimed(for: publicKeys)
            }

            let result = try await group.next() ?? false
            group.cancelAll()
            return result
        }
    }

    private func acquireHeadStream() -> HeadsSharedStream {
        headsWaiterCount += 1
        guard let headsSharedStream else {
            let stream = blockNumberProvider.subscribeFinalizedHeads().share()
            headsSharedStream = stream
            return stream
        }
        return headsSharedStream
    }

    private func releaseHeadStream() {
        headsWaiterCount -= 1
        guard headsWaiterCount == 0 else {
            return
        }
        headsSharedStream = nil
    }
}

// MARK: - Spent-coin recovery helpers

private extension TransferRecipientService {
    struct PreparedEntry {
        let index: Int
        let privateKey: Data
        let sourceCoin: OnChainCoin
        let destinationCoin: Coin
    }

    typealias SenderKey = (index: Int, privateKey: Data, publicKey: Data)
    typealias EntryFailure = (entryIndex: Int, error: Error)

    /// Flips spent coins whose on-chain age has reached `coinMaxAge` back to `.available`
    /// locally and refreshes their stored age. Returns the total planks of restored coins.
    func restoreMaxAgeCoins(
        spentCoins: [Coin],
        sourceCoins: [OnChainCoin?],
        context: DenominationBreakdownContext
    ) async throws -> BigUInt {
        var coinsToRestore: [Coin] = []
        var totalPlanks: BigUInt = .zero

        for (spent, source) in zip(spentCoins, sourceCoins) {
            guard let source, source.age >= CoinageConstants.coinMaxAge else { continue }
            coinsToRestore.append(spent.changing(isOnchain: true).changing(age: source.age))
            totalPlanks += context.valueInPlanks(for: spent.exponent)
        }

        if !coinsToRestore.isEmpty {
            try await coinService.save(coins: coinsToRestore)
            logger?.debug("Restored \(coinsToRestore.count) max-age spent coins to .available")
        }

        return totalPlanks
    }

    /// Drops entries already handled by `restoreMaxAgeCoins`. Externally-spent entries
    /// (nil source) stay so `allocateDestinations` can still count them.
    func filterTransferable(
        senderKeys: [SenderKey],
        sourceCoins: [OnChainCoin?]
    ) -> (keys: [SenderKey], sources: [OnChainCoin?]) {
        var keys: [SenderKey] = []
        var sources: [OnChainCoin?] = []

        for (key, source) in zip(senderKeys, sourceCoins) {
            if let source, source.age >= CoinageConstants.coinMaxAge {
                continue
            }
            keys.append(key)
            sources.append(source)
        }

        return (keys, sources)
    }

    func allocateDestinations(
        senderKeys: [SenderKey],
        sourceCoins: [OnChainCoin?],
        failures: inout [EntryFailure]
    ) async -> (prepared: [PreparedEntry], externallySpent: Int) {
        var prepared: [PreparedEntry] = []
        var externallySpent = 0

        for (entry, optCoin) in zip(senderKeys, sourceCoins) {
            guard let sourceCoin = optCoin else {
                externallySpent += 1
                continue
            }

            do {
                // The transfer we are about to submit ages the coin by one.
                let newCoin = try await coinMinter.mintCoin(
                    exponent: Int16(sourceCoin.value),
                    provenance: .received(
                        ageAfterTransfer: sourceCoin.age + 1,
                        bundleSize: senderKeys.count
                    )
                )
                prepared.append(PreparedEntry(
                    index: entry.index,
                    privateKey: entry.privateKey,
                    sourceCoin: sourceCoin,
                    destinationCoin: newCoin
                ))
            } catch {
                failures.append((entry.index, error))
            }
        }

        return (prepared, externallySpent)
    }

    func submitTransfers(
        _ entries: [PreparedEntry]
    ) async -> (claimed: [Coin], failures: [EntryFailure]) {
        var claimed: [Coin] = []
        var failures: [EntryFailure] = []

        await withTaskGroup(of: (Int, Result<Coin, Error>).self) { group in
            for entry in entries {
                group.addTask { [transferSubmitter, logger] in
                    do {
                        let coin = try await transferSubmitter.submitTransfer(
                            senderPrivateKey: entry.privateKey,
                            destinationCoin: entry.destinationCoin
                        )
                        logger?.debug("Transfer extrinsic succeeded for entry \(entry.index)")
                        return (entry.index, .success(coin))
                    } catch {
                        logger?.error("Transfer extrinsic failed for entry \(entry.index): \(error)")
                        return (entry.index, .failure(error))
                    }
                }
            }

            for await (index, result) in group {
                switch result {
                case let .success(coin):
                    claimed.append(coin)
                case let .failure(error):
                    failures.append((index, error))
                }
            }
        }

        return (claimed, failures)
    }
}

private extension String {
    /// Decodes a hex-encoded block number (e.g. a header's `number` field) into a decimal string.
    /// Falls back to the raw hex when the value is not valid hex or overflows `UInt32`.
    var hexBlockNumber: String {
        guard let number = UInt32.fromHex(self) else { return self }
        return String(number)
    }
}
