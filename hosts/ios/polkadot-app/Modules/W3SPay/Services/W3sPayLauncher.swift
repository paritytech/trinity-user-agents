import BackgroundExecution
import BigInt
import Coinage
import Foundation
import MessageExchangeKit
import SDKLogger
import StatementStore
import SubstrateOperation
import SubstrateSdk
import ChainRegistry

protocol W3sPayLaunching: AnyObject {
    @MainActor
    func launch(
        merchantKey: Data,
        topic: Data,
        paymentId: String,
        amount: W3sAmount,
        recipientLabel: String
    )
}

final class W3sPayLauncher: W3sPayLaunching {
    private let coinageService: CoinageServicing
    private let moduleNavigator: ModuleNavigating
    private let chainRegistry: ChainRegistryProtocol
    private let statementStoreChainId: ChainModel.Id
    private let mainChainAssetId: ChainAssetId
    private let historyStore: W3sPaymentHistoryStoring
    private let walletRepo: WalletManagerRepositoryProtocol
    private let logger: SDKLoggerProtocol?

    init(
        coinageService: CoinageServicing,
        moduleNavigator: ModuleNavigating = ModuleNavigator(),
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        statementStoreChainId: ChainModel.Id = AppConfig.Chains.chatChain,
        mainChainAssetId: ChainAssetId = AppConfig.Assets.mainAsset,
        historyStore: W3sPaymentHistoryStoring,
        walletRepo: WalletManagerRepositoryProtocol = .shared,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.coinageService = coinageService
        self.moduleNavigator = moduleNavigator
        self.chainRegistry = chainRegistry
        self.statementStoreChainId = statementStoreChainId
        self.mainChainAssetId = mainChainAssetId
        self.historyStore = historyStore
        self.walletRepo = walletRepo
        self.logger = logger
    }

    @MainActor
    func launch(
        merchantKey: Data,
        topic: Data,
        paymentId: String,
        amount: W3sAmount,
        recipientLabel: String
    ) {
        guard
            let chain = chainRegistry.getChain(for: mainChainAssetId.chainId),
            let chainAsset = chain.chainAsset(for: mainChainAssetId.assetId),
            let amountInPlanks = amount.decimal.toSubstrateAmount(
                precision: chainAsset.asset.decimalPrecision
            )
        else {
            logger?.error("W3S launch: failed to resolve chain asset or convert amount")
            return
        }

        let details = W3sPaymentDetails(
            paymentId: paymentId,
            topic: topic,
            merchantKey: merchantKey,
            merchantName: recipientLabel.isEmpty ? nil : recipientLabel,
            amountString: amount.normalizedString,
            chainAssetId: "\(mainChainAssetId.chainId)-\(mainChainAssetId.assetId)"
        )

        guard let submitter = makeSubmitter(details: details, historyStore: historyStore) else {
            return
        }

        // accountId is a per-payment placeholder (topic); the row renders the
        // recipientLabel via `username`, never the synthetic SS58 address.
        let recipient = RecipientModel(accountId: topic, username: recipientLabel)

        let lifecycleReporter = W3sPaymentLifecycleReporter(
            historyStore: historyStore,
            paymentId: paymentId
        )

        guard let view = TransferAmountViewFactory.createCoinsViaStatementStore(
            for: chainAsset,
            recipient: recipient,
            coinageService: coinageService,
            chatSubmitter: submitter,
            amountInPlanks: amountInPlanks,
            lifecycleReporter: lifecycleReporter
        ) else {
            return
        }

        moduleNavigator.presentModally(view.controller)
    }
}

private extension W3sPayLauncher {
    func makeSubmitter(
        details: W3sPaymentDetails,
        historyStore: W3sPaymentHistoryStoring
    ) -> W3sStatementSubmitter? {
        do {
            let connection = try chainRegistry.getConnectionOrError(for: statementStoreChainId)
            let statementSubmitter = StatementStoreConnection(
                connection: connection,
                retryMatcher: StatementSubmitErrorMatcher.retryWhenTimeoutOrNoAllowance(),
                logger: logger
            )

            let blockInfoProvider = BlockInfoProvider(
                chainRegistry: chainRegistry,
                operationQueue: OperationManagerFacade.sharedDefaultQueue,
                chainId: mainChainAssetId.chainId
            )

            return try W3sStatementSubmitter(
                details: details,
                wallet: walletRepo.main(),
                statementStoreSubmitter: statementSubmitter,
                historyStore: historyStore,
                blockInfoProvider: blockInfoProvider,
                backgroundExecutor: ConnectionRetainingExecutor(provider: chainRegistry),
                logger: logger
            )
        } catch {
            logger?.error("W3S launch: failed to prepare statement-store submitter: \(error)")
            return nil
        }
    }
}
