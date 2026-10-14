#if TESTNET_FEATURE
    import Foundation
    import Operation_iOS
    import ChainRegistry

    protocol AppFactoryResetCheckerFactoryProtocol {
        func makeChecker(chainRegistry: ChainRegistryProtocol) -> AppFactoryResetChecker
    }

    struct AppFactoryResetCheckerFactory: AppFactoryResetCheckerFactoryProtocol {
        let operationQueue: OperationQueue
        let usernameChain: ChainModel.Id

        func makeChecker(chainRegistry: ChainRegistryProtocol) -> AppFactoryResetChecker {
            let identityService = IdentityService(
                chainRegistry: chainRegistry,
                chain: usernameChain,
                operationQueue: operationQueue,
                logger: Logger.shared
            )

            return AppFactoryResetChecker(
                storage: UsernameStorage(),
                walletRepo: .shared,
                identityService: identityService
            )
        }
    }
#endif
