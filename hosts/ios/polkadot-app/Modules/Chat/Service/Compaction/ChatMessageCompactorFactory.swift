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
    let signerProvider: AttachmentsSenderProviding
    let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        allowanceManager: AllowanceManaging,
        signerProvider: AttachmentsSenderProviding = AttachmentsSenderProvider(),
        logger: LoggerProtocol = Logger.shared
    ) {
        hopNodeProvider = HOPNodeProvider(chainRegistry: chainRegistry)
        hopLoaderFactory = HOPFileLoaderFactory(logger: logger)
        self.allowanceManager = allowanceManager
        self.signerProvider = signerProvider
        self.logger = logger
    }
}

extension ChatMessageCompactorFactory: ChatMessageCompactorMaking {
    func createCompactor() throws -> AnyMessageCompactor<Chat.OpaqueMessage>? {
        let compactor = ChatMessageCompactor(
            nodeProvider: hopNodeProvider,
            loaderFactory: hopLoaderFactory,
            signerProvider: signerProvider,
            allowanceManager: allowanceManager,
            logger: logger
        )

        return AnyMessageCompactor(compactor)
    }
}
