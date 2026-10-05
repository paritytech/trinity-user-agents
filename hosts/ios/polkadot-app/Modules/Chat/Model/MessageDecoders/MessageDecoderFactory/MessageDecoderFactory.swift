import Foundation
import ChainRegistry

protocol ChatMessageDecoderMaking {
    func makeDecoders(for chain: ChainModel, chatId: Chat.Id) -> [ChatMessageCustomDecoding]
}

final class ChatMessageDecoderFactory: ChatMessageDecoderMaking {
    private let extensionsRegistry: ChatExtensionsRegistering

    init(extensionsRegistry: ChatExtensionsRegistering) {
        self.extensionsRegistry = extensionsRegistry
    }

    func makeDecoders(for chain: ChainModel, chatId: Chat.Id) -> [ChatMessageCustomDecoding] {
        let staticDecoders: [ChatMessageCustomDecoding] = [
            GameDepositMessageDecoder(chain: chain)
        ] + makeCommonDecoders()

        let extensionDecoders = extensionsRegistry
            .getExtensions(for: chatId)
            .flatMap(\.customDecoders)

        return staticDecoders + extensionDecoders
    }
}

private extension ChatMessageDecoderFactory {
    func makeCommonDecoders() -> [ChatMessageCustomDecoding] {
        []
    }
}
