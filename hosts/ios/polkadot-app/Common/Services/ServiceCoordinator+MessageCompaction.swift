import Foundation

extension ServiceCoordinator {
    static func createCompactedMessageExpansionService(
        logger: LoggerProtocol
    ) -> CompactedMessageExpansionServicing {
        let hopNodeProvider = HOPNodeProvider(chainRegistry: ChainRegistryFacade.sharedRegistry)
        let hopLoaderFactory = HOPFileLoaderFactory(logger: logger)

        let claimer = ChatMessageClaimer(
            nodeProvider: hopNodeProvider,
            loaderFactory: hopLoaderFactory,
            logger: logger
        )

        let context = CompactedMessageExpansionContext(
            storageFacade: UserDataStorageFacade.shared,
            logger: logger
        )

        return CompactedMessageExpansionService(
            claimer: claimer,
            messageProviderFactory: ChatMessageDataProviderFactory(),
            context: context,
            logger: logger
        )
    }
}
