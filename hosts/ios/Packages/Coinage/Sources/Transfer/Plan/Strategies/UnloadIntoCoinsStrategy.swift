import DurableTransactions
import Foundation
import FoundationExt
import ExtrinsicService
import StructuredConcurrency
import SubstrateSdk
import SubstrateSdkExt
import KeyDerivation
import BigInt
import SDKLogger
import SubstrateOperation

/// Strategy 3: Unload vouchers directly into required denominations.
///
/// Declares one transaction per planned call. `CoinSelector` sizes the calls so each respects the
/// `MaxConsolidation` and `MaxSplitOutputs` pallet constraints, which can put several calls on one
/// recycler. The pallet marks aliases individually and leaves the ring revision untouched, so calls
/// sharing a recycler do not conflict — they are built and submitted independently by the unload
/// policy, and each call's vouchers are fixed by the row it was registered as.
struct UnloadIntoCoinsStrategy {
    private let readyCoins: [Coin]
    private let perGroupAllocations: [RecyclerGroupAllocation]
    private let minter: any CoinMinting
    private let txService: any CoinageTxServicing
    private let dateProvider: any DateProviding
    private let logger: SDKLoggerProtocol?

    init(
        readyCoins: [Coin],
        perGroupAllocations: [RecyclerGroupAllocation],
        minter: any CoinMinting,
        txService: any CoinageTxServicing,
        dateProvider: any DateProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.readyCoins = readyCoins
        self.perGroupAllocations = perGroupAllocations
        self.minter = minter
        self.txService = txService
        self.dateProvider = dateProvider
        self.logger = logger
    }
}

// MARK: - TransferStrategy

extension UnloadIntoCoinsStrategy: TransferStrategy {
    func prepare() async throws -> PreparedStrategy {
        guard !perGroupAllocations.isEmpty else {
            throw TransferStrategyError.emptyVouchers
        }

        let allVouchers = perGroupAllocations.flatMap(\.vouchers)

        guard !allVouchers.contains(where: { $0.recycler == nil }) else {
            throw TransferStrategyError.missingRecyclerInfo
        }

        var realizedGroups: [RecyclerGroupCoins] = []
        for allocation in perGroupAllocations {
            // Every voucher in an allocation sits in the same recycler, so they carry the same
            // score; the minimum is a tie-break for readings taken a tick apart.
            let provenance = CoinProvenance.unloaded(
                recyclerFungibility: allocation.vouchers.map(\.recyclerFungibility).min()
            )

            let recipientCoins = try await minter
                .mintCoins(allocation.recipientDenominations.map(\.exponent), provenance: provenance)
            let changeCoins = try await minter
                .mintCoins(allocation.changeDenominations.map(\.exponent), provenance: provenance)
            realizedGroups.append(RecyclerGroupCoins(
                recyclerKey: allocation.recyclerKey,
                vouchers: allocation.vouchers,
                recipientCoins: recipientCoins,
                changeCoins: changeCoins
            ))
        }

        // Declared, not built. The slow part of an unload — resolving a free token, pinning a block,
        // proving each voucher — happens when the policy builds it, after the memo has already left.
        // The token and the recycler revision are deliberately not resolved here: they would be stale
        // by the time the transaction is built, and a token reserved for a build that never happened
        // is a token spent for nothing.
        // The retry window opens now, when the transactions are declared, not when the plan was made.
        let retryFrom = await dateProvider.read()
        let scheduled = try realizedGroups.map { group in
            try CoinageScheduledTxRequest(
                policy: CoinageSubmissionParams.unloadPolicy(.retriedTransfer(from: retryFrom)),
                inputs: group.vouchers.map { .recyclerVoucher($0.derivationIndex, $0.publicKey) },
                outputs: (group.recipientCoins + group.changeCoins)
                    .map { .coin($0.derivationIndex, $0.publicKey) }
            )
        }

        logger?.info("Declared \(scheduled.count) unload(s) for \(allVouchers.count) vouchers")

        // Ready coins need no submission; every group's recipient coins leave to the peer. Change
        // coins stay ours. All pre-committed before the memo can leave.
        let handedOff = readyCoins + realizedGroups.flatMap(\.recipientCoins)
        let handoffCommit = try await txService
            .preCommitHandoff(handedOff.map { .coin($0.derivationIndex, $0.publicKey) })

        var memoEntries = readyCoins.map {
            PlannedMemoEntry(
                coinDerivationIndex: $0.derivationIndex,
                valueExponent: $0.exponent
            )
        }
        for group in realizedGroups {
            memoEntries += group.recipientCoins.map {
                PlannedMemoEntry(
                    coinDerivationIndex: $0.derivationIndex, valueExponent: $0.exponent
                )
            }
        }

        return PreparedStrategy(
            memoEntries: memoEntries,
            handoffCommit: handoffCommit,
            transactions: scheduled
        )
    }
}
