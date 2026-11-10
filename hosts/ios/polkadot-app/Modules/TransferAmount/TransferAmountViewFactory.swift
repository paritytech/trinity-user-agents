import Foundation
import BigInt
import ExtrinsicService
import SubstrateSdk
import Coinage
import ChainRegistry

@MainActor
enum TransferAmountViewFactory {
    static func createChatTransfer(
        for chainAsset: ChainAsset,
        recipient: RecipientModel,
        coinageService: CoinageServicing
    ) -> TransferAmountViewProtocol? {
        let wireframe = ChatTransferAmountWireframe(chainAsset: chainAsset)
        return createView(
            for: chainAsset,
            recipient: recipient,
            wireframe: wireframe,
            transferMethod: .coinage,
            coinageService: coinageService,
            chatSubmitter: makeContactChatSubmitter()
        )
    }

    static func createTransfer(
        for chainAsset: ChainAsset,
        recipient: RecipientModel,
        coinageService: CoinageServicing
    ) -> TransferAmountViewProtocol? {
        let wireframe = TransferAmountWireframe(recipient: recipient)
        return createView(
            for: chainAsset,
            recipient: recipient,
            wireframe: wireframe,
            transferMethod: .coinage,
            coinageService: coinageService,
            chatSubmitter: makeContactChatSubmitter()
        )
    }

    static func createExternalPayment(
        for chainAsset: ChainAsset,
        recipient: RecipientModel,
        coinageService: CoinageServicing,
        amountInPlanks: BigUInt,
        lockAmount: Bool
    ) -> TransferAmountViewProtocol? {
        let wireframe = SuccessPaymentWireframe()
        let config = TransferAmountConfig(
            prefilledAmountInPlanks: amountInPlanks,
            isAmountLocked: lockAmount,
            requiresExactAmount: true,
            recipientIsPlaceholder: false
        )

        return createView(
            for: chainAsset,
            recipient: recipient,
            wireframe: wireframe,
            transferMethod: .externalPayment,
            coinageService: coinageService,
            chatSubmitter: NoChatSubmitter(),
            config: config,
            lifecycleReporter: ExternalPaymentLifecycleReporter(coinageService: coinageService)
        )
    }

    /// TransferAmount with a locked amount and a caller-supplied submitter
    /// (W3S Statement Store delivery instead of an in-chat memo).
    static func createCoinsViaStatementStore(
        for chainAsset: ChainAsset,
        recipient: RecipientModel,
        coinageService: CoinageServicing,
        chatSubmitter: TransferSubmitting,
        amountInPlanks: BigUInt,
        lifecycleReporter: TransferLifecycleReporting = NoOpTransferLifecycleReporter()
    ) -> TransferAmountViewProtocol? {
        let wireframe = SuccessPaymentWireframe()
        let config = TransferAmountConfig(
            prefilledAmountInPlanks: amountInPlanks,
            isAmountLocked: true,
            requiresExactAmount: true,
            recipientIsPlaceholder: true
        )

        return createView(
            for: chainAsset,
            recipient: recipient,
            wireframe: wireframe,
            transferMethod: .coinage,
            coinageService: coinageService,
            chatSubmitter: chatSubmitter,
            config: config,
            lifecycleReporter: lifecycleReporter
        )
    }

    private static func createView(
        for chainAsset: ChainAsset,
        recipient: RecipientModel,
        wireframe: TransferAmountWireframeProtocol,
        transferMethod: TransferMethod,
        coinageService: CoinageServicing,
        chatSubmitter: TransferSubmitting,
        config: TransferAmountConfig = .default,
        lifecycleReporter: TransferLifecycleReporting = NoOpTransferLifecycleReporter()
    ) -> TransferAmountViewProtocol? {
        guard let interactor = createInteractor(
            for: chainAsset,
            recipient: recipient,
            transferMethod: transferMethod,
            coinageService: coinageService,
            chatSubmitter: chatSubmitter,
            lifecycleReporter: lifecycleReporter
        ) else {
            return nil
        }
        let displayInfo = chainAsset.asset.digitalDollarDisplayInfo
        let balanceViewModelFactory = BalanceViewModelFactory(
            targetAssetInfo: displayInfo
        )
        let availableBalanceViewModelFactory = BalanceViewModelFactory(
            targetAssetInfo: displayInfo.withFiatSymbol
        )

        let inputStrategy = AmountInputTokenStrategy(
            chainAsset: displayInfo.withFiatSymbol,
            balanceViewModelFactory: balanceViewModelFactory
        )

        let dataValidatorFactory = TransferDataValidatorFactory(
            presentable: wireframe,
            chainAsset: chainAsset
        )

        let presenter = TransferAmountPresenter(
            interactor: interactor,
            wireframe: wireframe,
            chainAsset: chainAsset,
            balanceViewModelFactory: balanceViewModelFactory,
            availableBalanceViewModelFactory: availableBalanceViewModelFactory,
            amountInputStrategy: inputStrategy,
            dataValidationFactory: dataValidatorFactory,
            config: config
        )

        let view = TransferAmountViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter
        dataValidatorFactory.view = view

        return view
    }

    private static func createInteractor(
        for chainAsset: ChainAsset,
        recipient: RecipientModel,
        transferMethod: TransferMethod,
        coinageService: CoinageServicing,
        chatSubmitter: TransferSubmitting,
        lifecycleReporter: TransferLifecycleReporting = NoOpTransferLifecycleReporter()
    ) -> TransferAmountInteractor? {
        let queue = OperationManagerFacade.sharedDefaultQueue
        let walletRepo: WalletManagerRepositoryProtocol = .shared

        guard let wallet = try? walletRepo.main() else {
            return nil
        }

        let logger = Logger.shared

        let repositoryFactory = RecentContactRepositoryFactory(
            storageFacade: UserDataStorageFacade.shared
        )
        let contactsRepository = repositoryFactory.createRecentContactsRepository()

        let dependencies = TransferAmountDependency(
            wallet: { wallet },
            recipient: { recipient },
            chainAsset: { chainAsset },
            contactsRepository: { contactsRepository },
            operationQueue: { queue },
            coinageService: { coinageService },
            transferMethod: { transferMethod },
            chatSubmitter: { chatSubmitter },
            lifecycleReporter: { lifecycleReporter }
        )

        return try? TransferAmountInteractor(
            dependencies: dependencies,
            logger: logger
        )
    }

    private static func makeContactChatSubmitter() -> TransferSubmitting {
        ContactChatSubmitter(
            chatContactsProvider: ContactsLocalStorageService(),
            createMessageFactory: LocalMessageCreatingOperationFactory(
                messagesStorageService: MessagesLocalStorageService()
            ),
            messageStore: ChatMessageTransactionalStore(storageFacade: UserDataStorageFacade.shared),
            logger: Logger.shared
        )
    }
}
