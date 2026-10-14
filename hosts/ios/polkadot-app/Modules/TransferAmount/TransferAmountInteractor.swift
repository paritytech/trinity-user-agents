import AsyncExtensions
import Foundation
import Operation_iOS
import ExtrinsicService
import SubstrateSdk
import SDKLogger
import OperationExt
import Coinage
import KeyDerivation
import StructuredConcurrency
import BigInt
import ChainRegistry
import SubstrateSdkExt

#if TESTNET_FEATURE
    import Keystore_iOS
#endif

struct TransferAmountDependency {
    let wallet: () -> WalletManaging
    let recipient: () -> RecipientModel
    let chainAsset: () -> ChainAsset
    let contactsRepository: () -> AnyDataProviderRepository<RecentContactModel>
    let operationQueue: () -> OperationQueue
    let coinageService: () -> CoinageServicing
    let transferMethod: () -> TransferMethod
    let chatSubmitter: () -> TransferSubmitting
    var lifecycleReporter: () -> TransferLifecycleReporting = { NoOpTransferLifecycleReporter() }
    var recyclingStrategy: () -> any CoinageRecyclingStrategyProviding = { CoinageRecyclingStrategyStore.shared }
}

final class TransferAmountInteractor {
    weak var presenter: TransferAmountInteractorOutputProtocol?

    let wallet: WalletManaging
    let accountId: AccountId
    let recipient: RecipientModel
    let chainAsset: ChainAsset
    let operationQueue: OperationQueue

    let contactsRepository: AnyDataProviderRepository<RecentContactModel>
    let logger: SDKLoggerProtocol?
    let coinageService: CoinageServicing
    let transferMethod: TransferMethod
    let transferSubmitter: TransferSubmitting
    let lifecycleReporter: TransferLifecycleReporting
    let recyclingStrategy: any CoinageRecyclingStrategyProviding

    private var coinageBalanceTask: Task<Void, Never>?
    /// Coalesces concurrent confirmations (e.g. double-tap): late callers join
    /// the in-flight submission and receive its real outcome.
    private let confirmCall = CoalescingTask<Void>()

    init(
        dependencies: TransferAmountDependency,
        logger: SDKLoggerProtocol?
    ) throws {
        wallet = dependencies.wallet()
        accountId = try wallet.getRawPublicKey()
        recipient = dependencies.recipient()
        chainAsset = dependencies.chainAsset()
        operationQueue = dependencies.operationQueue()
        contactsRepository = dependencies.contactsRepository()
        coinageService = dependencies.coinageService()
        transferMethod = dependencies.transferMethod()
        transferSubmitter = dependencies.chatSubmitter()
        lifecycleReporter = dependencies.lifecycleReporter()
        recyclingStrategy = dependencies.recyclingStrategy()
        self.logger = logger
    }
}

// MARK: - TransferAmountInteractorInputProtocol

extension TransferAmountInteractor: TransferAmountInteractorInputProtocol {
    var senderAddress: AccountId {
        accountId
    }

    func lifecycleStream() -> AnyAsyncSequence<OutgoingTransferState> {
        lifecycleReporter.makeStream()
    }

    func setup() {
        startCoinageBalanceObservation()
    }

    func retrySetup() {
        startCoinageBalanceObservation()
    }

    func previewTransfer(for amount: Decimal) async throws -> TransferPreviewValidation {
        guard let planks = chainAsset.asset.planks(from: amount) else {
            throw TransferAmountInteractorError.internalError
        }

        switch transferMethod {
        case .coinage:
            let preview = try await coinageService.previewTransfer(for: planks)
            return .coinage(preview)
        case .externalPayment:
            let preview = try await coinageService.previewExternalPayment(for: planks)
            // Same rule as product payments: warn unless private vouchers pay or the preset is minPrivacy.
            var requiresPrivacyConfirmation = false
            if recyclingStrategy.strategy != .minPrivacy {
                requiresPrivacyConfirmation = try await !coinageService
                    .canExecuteExternalPaymentPrivately(amount: planks)
            }
            return .externalPayment(preview, amount: planks, requiresPrivacyConfirmation: requiresPrivacyConfirmation)
        }
    }

    func confirmTransfer(validation: TransferPreviewValidation) async throws {
        coinageBalanceTask?.cancel()
        coinageBalanceTask = nil

        try await confirmCall.run { [self] in
            do {
                switch validation {
                case let .coinage(preview):
                    try await confirmCoinageTransfer(preview: preview)
                case let .externalPayment(_, amount, _):
                    try await confirmExternalPayment(amount: amount)
                }
            } catch {
                logger?.error("Did fail transfer: \(error)")

                retrySetup()
                throw TransferAmountInteractorError.transactionFailed(error)
            }
        }
    }

    func saveRecentContact() {
        let contact = RecentContactModel(
            accountID: recipient.accountId,
            chainAssetID: chainAsset.chainAssetId
        )
        let saveOperation = contactsRepository.saveOperation {
            [contact]
        } _: {
            []
        }
        execute(
            operation: saveOperation,
            inOperationQueue: operationQueue,
            runningCallbackIn: .main
        ) { [weak self] result in
            switch result {
            case .success:
                self?.logger?.info("Recent Contact successfully saved in the database")
            case let .failure(error):
                self?.logger?.debug(error.localizedDescription)
            }
        }
    }
}

// MARK: - Coinage Transfer

private extension TransferAmountInteractor {
    func confirmCoinageTransfer(preview: TransferPreview) async throws {
        let result = preview.selectionResult
        // One id shared by the coinage transactions (their groupId) and the chat message that
        // carries the memo, so the transfer's on-chain work and its message correlate.
        let messageId: Chat.MessageId = UUID().uuidString
        // Mints and reserves only: nothing is built and nothing is on the wire yet, so the slow part
        // of a payment no longer stands between the user and the memo leaving.
        let prepared = try await coinageService.executeTransfer(result: result, groupId: messageId)
        do {
            // The hook runs inside the transaction that persists the memo, so the handoff becomes
            // final and the payment's transactions are registered exactly when the keys are durable.
            try await transferSubmitter.sendTransfer(
                prepared.memo,
                to: recipient.accountId,
                messageId: messageId
            ) { scope in
                try prepared.commit(in: scope)
            }
        } catch {
            // Every transport runs the hook inside a transaction, and only once whatever carries the
            // keys is durable — so a throw from here means nothing was committed and nothing scheduled.
            // Drop the reservation now rather than waiting for a relaunch, and never report a transfer
            // the recipient has no way to claim.
            try? await prepared.abandon()

            throw error
        }
        lifecycleReporter.start(with: .coinageMemo(prepared.memo))
    }
}

// MARK: - External Payment

private extension TransferAmountInteractor {
    /// Reaches here only after the presenter's privacy confirmation when the plan requires one.
    func confirmExternalPayment(amount: BigUInt) async throws {
        let productId = ExternalPayment.nativeProductId
        let paymentId = try Data.randomOrError(of: 32).toHex(includePrefix: true)

        try await coinageService.initiateExternalPayment(
            productId: productId,
            paymentId: paymentId,
            amountInPlanks: amount,
            destination: recipient.accountId
        )

        // Completion and failure are observed by the presenter through the
        // lifecycle stream — initiation success is enough to return here.
        lifecycleReporter.start(
            with: .externalPayment(productId: productId, paymentId: paymentId, amountInPlanks: amount)
        )
    }
}

// MARK: - Balance Observation

private extension TransferAmountInteractor {
    func startCoinageBalanceObservation() {
        coinageBalanceTask?.cancel()
        let service = coinageService
        coinageBalanceTask = Task { [weak self] in
            do {
                let balanceService = try await service.coinageBalanceService()
                for try await balance in balanceService.balanceStream {
                    // `availablePrivate` is spendable at no privacy cost; `gainingPrivacy` is the
                    // funds this strategy would still release behind a confirmation (none under max
                    // privacy). Together they form the reachable amount.
                    let gainingPrivacy = balance.gainingPrivacy.canSpendWithConfirmation
                        ? balance.gainingPrivacy.amount
                        : 0
                    let breakdown = TransferSpendableBreakdown(
                        availablePrivate: balance.availablePrivate,
                        gainingPrivacy: gainingPrivacy
                    )
                    await self?.presenter?.didReceive(spendableBreakdown: breakdown)
                }
            } catch {
                self?.logger?.error("Failed to observe coinage balance: \(error)")
            }
        }
    }
}

// MARK: - DEBUG

extension TransferAmountInteractor {
    #if TESTNET_FEATURE
        func previewStrategy(for amount: Decimal) {
            // No need to properly inject settings here as this is a debug view
            guard SettingsManager.shared.bool(for: SettingsKey.showTransferStrategyDebug.rawValue) ?? true else {
                Task { @MainActor [weak self] in
                    self?.presenter?.didReceive(strategyDebugInfo: nil)
                }
                return
            }

            let service = coinageService
            let method = transferMethod
            Task { [weak self, chainAsset] in
                do {
                    guard let plank = chainAsset.asset.planks(from: amount) else {
                        throw TransferAmountInteractorError.internalError
                    }

                    let fetcher: TransferDebugInfoFetching =
                        switch method {
                        case .coinage:
                            CoinageDebugInfoFetcher(coinageService: service)
                        case .externalPayment:
                            ExternalPaymentDebugInfoFetcher(coinageService: service)
                        }

                    let debugInfo = try await fetcher.fetchDebugInfo(for: plank)
                    await self?.presenter?.didReceive(strategyDebugInfo: debugInfo)
                } catch {
                    self?.logger?.error("Strategy preview failed: \(error)")
                    await self?.presenter?.didReceive(strategyDebugInfo: nil)
                }
            }
        }
    #endif
}

private extension AssetModel {
    func planks(from decimal: Decimal) -> BigUInt? {
        decimal.toSubstrateAmount(precision: decimalPrecision)
    }
}
