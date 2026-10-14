import Foundation
import MessageExchangeKit
import ChainRegistry
import KeyDerivation
import Individuality

protocol ChatMessageCompactorMaking: MessageCompactorMaking
    where Message == Chat.OpaqueMessage {}

final class ChatMessageCompactorFactory: @unchecked Sendable {
    let hopNodeProvider: HOPNodeProviding
    let hopLoaderFactory: HOPFileLoaderMaking
    let allowanceManager: AllowanceManaging
    let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        allowanceManager: AllowanceManaging,
        logger: LoggerProtocol = Logger.shared
    ) {
        hopNodeProvider = HOPNodeProvider(chainRegistry: chainRegistry)
        hopLoaderFactory = HOPFileLoaderFactory(logger: logger)
        self.allowanceManager = allowanceManager
        self.logger = logger
    }
}

extension ChatMessageCompactorFactory: ChatMessageCompactorMaking {
    func createCompactor(for signKeyId: String) -> AnyMessageCompactor<Chat.OpaqueMessage>? {
        let proofWallet = DynamicDerivedWallet(derivationPath: signKeyId)

        let compactor = ChatMessageCompactor(
            nodeProvider: hopNodeProvider,
            loaderFactory: hopLoaderFactory,
            proofWallet: proofWallet,
            allowanceManager: allowanceManager,
            logger: logger
        )

        return AnyMessageCompactor(compactor)
    }
}
