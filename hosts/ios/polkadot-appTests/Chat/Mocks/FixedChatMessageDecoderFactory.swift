import ChainRegistry
import Foundation
@testable import polkadot_app

/// Gives the same decoders for every chain and chat.
struct FixedChatMessageDecoderFactory: ChatMessageDecoderMaking {
    let decoders: [ChatMessageCustomDecoding]

    func makeDecoders(for _: ChainModel, chatId _: Chat.Id) -> [ChatMessageCustomDecoding] {
        decoders
    }
}
