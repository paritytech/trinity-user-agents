import Foundation
import Products
import AsyncExtensions

// MARK: - Messaging

extension ProductsNativeApi: BoundProductChatMessaging {}

// MARK: - Helpers

extension ProductBotMessage {
    func toChatMessageContent() -> Chat.LocalMessage.Content {
        switch self {
        case let .text(text):
            .text(text)
        case let .custom(messageType, data, alt):
            .customRendered(
                Chat.LocalMessage.Content.CustomRenderedData(
                    decoderId: MessageDecoderIdentifier.product.rawValue,
                    data: data,
                    identifier: messageType,
                    alt: alt
                )
            )
        }
    }
}
