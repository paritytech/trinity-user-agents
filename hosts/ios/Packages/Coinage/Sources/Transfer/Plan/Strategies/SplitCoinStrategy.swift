import Foundation
import FoundationExt
import KeyDerivation
import ExtrinsicService
import StructuredConcurrency
import SubstrateSdk
import SubstrateSdkExt
import SDKLogger

/// Strategy 2: split one coin into recipient and change denominations.
///
/// Only `overflowCoin` is consumed on chain, so it is the entry's single input — which is what
/// Appendix B says a split takes. `wholeCoins` are passed to the recipient untouched and are
/// recorded as handoffs rather than as inputs.
///
/// Both the recipient coins and the change coins are declared as outputs. Recording the
/// recipient side matters: an entry that declares only its change has no evidence left once
/// the peer claims, and cannot be told apart from one that never executed.
struct SplitCoinStrategy {
    private let wholeCoins: [Coin]
    private let overflowCoin: Coin
    private let targetDenominations: [Denomination]
    private let changeDenominations: [Denomination]
    private let minter: any CoinMinting
    private let txService: any CoinageTxServicing
    private let dateProvider: any DateProviding
    private let logger: SDKLoggerProtocol?

    init(
        wholeCoins: [Coin],
        overflowCoin: Coin,
        targetDenominations: [Denomination],
        changeDenominations: [Denomination],
        minter: any CoinMinting,
        txService: any CoinageTxServicing,
        dateProvider: any DateProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.wholeCoins = wholeCoins
        self.overflowCoin = overflowCoin
        self.targetDenominations = targetDenominations
        self.changeDenominations = changeDenominations
        self.minter = minter
        self.txService = txService
        self.dateProvider = dateProvider
        self.logger = logger
    }
}

// MARK: - TransferStrategy

extension SplitCoinStrategy: TransferStrategy {
    func prepare() async throws -> PreparedStrategy {
        // Every piece of the split shares one provenance: the overflow coin's chain, plus this
        // split. Fanout counts all outputs, the recipient's and ours alike, since that is how many
        // ways the input was divided.
        let provenance = CoinProvenance.split(
            from: overflowCoin,
            fanout: targetDenominations.count + changeDenominations.count
        )

        let recipientCoins = try await minter.mintCoins(
            targetDenominations.map(\.exponent),
            provenance: provenance
        )
        // Change coins stay ours — minted as outputs but not handed off.
        let changeCoins = try await minter.mintCoins(
            changeDenominations.map(\.exponent),
            provenance: provenance
        )

        var transaction = CoinageTransaction()
        transaction.mint(coins: recipientCoins + changeCoins)
        transaction.consume(coins: [overflowCoin])
        // Recipient coins are handed off before submit: a key that reaches the recipient without a
        // mark could be selected again. Change coins stay ours and are not handed off.
        transaction.handOff(coins: wholeCoins + recipientCoins)
        let assets = transaction.build()

        // Declared, not built: the call and its origin are reconstructed from these same assets by
        // `SplitRebuild` when the policy builds it, so nothing here needs an unload token or a proof.
        // The retry window opens now, when the transaction is declared, not when the plan was made.
        let scheduled = try await CoinageScheduledTxRequest(
            policy: CoinageSubmissionParams.splitPolicy(.retriedTransfer(from: dateProvider.read())),
            inputs: assets.inputs,
            outputs: assets.outputs
        )

        let handoffCommit = try await txService
            .preCommitHandoff(assets.handedOff.map { .coin($0.derivationIndex, $0.publicKey) })

        var memoEntries = wholeCoins.map {
            PlannedMemoEntry(
                coinDerivationIndex: $0.derivationIndex,
                valueExponent: $0.exponent
            )
        }
        memoEntries += recipientCoins.map {
            PlannedMemoEntry(coinDerivationIndex: $0.derivationIndex, valueExponent: $0.exponent)
        }

        return PreparedStrategy(
            memoEntries: memoEntries,
            handoffCommit: handoffCommit,
            transactions: [scheduled]
        )
    }
}
