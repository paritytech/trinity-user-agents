import struct TrUAPIHost.ProductId
import AsyncExtensions
import Foundation
import Operation_iOS
import SubstrateSdk
import SDKLogger
import Coinage
import KeyDerivation
import StructuredConcurrency
import BigInt
import ChainRegistry

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
            // Same rule as product payments: warn unless private value pays or the preset is minPrivacy.
            let requiresPrivacyConfirmation = recyclingStrategy.strategy != .minPrivacy && preview != .privately
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
                case let .externalPayment(preview, amount, _):
                    try await confirmExternalPayment(amount: amount, confirmed: preview != .privately)
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
        // One id for the chat message that carries the memo, from which the export's id is derived, so
        // the payment's export and its message correlate.
        let messageId: Chat.MessageId = UUID().uuidString
        let retryUntil = Date().addingTimeInterval(CoinageConstants.chatPaymentRetryWindow)
        let prepared = try await coinageService.prepareTransfer(
            preview,
            exportId: transferSubmitter.exportId(messageId: messageId),
            retryUntil: retryUntil
        )
        do {
            try await transferSubmitter.sendTransfer(
                prepared.memo,
                to: recipient.accountId,
                messageId: messageId
            ) {
                try await prepared.commit()
            }
        } catch {
            // The memo may not have left: take the coins back, which loses nothing, since whoever moves
            // a coin first owns it.
            do {
                try await prepared.abandon(reclaimId: .chatReclaim(messageId: messageId), retryUntil: retryUntil)
            } catch {
                logger?.error("Failed to take back an unsent payment: \(error)")
            }

            throw error
        }
        lifecycleReporter.start(with: .coinageMemo(prepared.memo))
    }
}

// MARK: - External Payment

private extension TransferAmountInteractor {
    /// Reaches here only after the presenter's privacy confirmation when the plan requires one.
    func confirmExternalPayment(amount: BigUInt, confirmed: Bool) async throws {
        let productId: ProductId? = nil
        let paymentId = try Data.randomOrError(of: 32).toHex(includePrefix: true)

        try await coinageService.initiateExternalPayment(
            ExternalPaymentRequest(
                productId: productId,
                paymentId: paymentId,
                amountInPlanks: amount,
                destination: recipient.accountId,
                confirmed: confirmed
            )
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
                for try await balance in service.balanceStream() {
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

private extension AssetModel {
    func planks(from decimal: Decimal) -> BigUInt? {
        decimal.toSubstrateAmount(precision: decimalPrecision)
    }
}
