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
        case let .richText(text, media):
            .productBody(.richText(.init(text: text, media: media)))
        case let .actions(actions):
            .productBody(.actions(.init(
                text: actions.text,
                buttons: actions.buttons.map { .init(actionId: $0.actionId, title: $0.title) },
                layout: actions.layout.toLocal()
            )))
        case let .file(file):
            .productBody(.file(.init(
                url: file.url,
                fileName: file.fileName,
                mimeType: file.mimeType,
                sizeBytes: file.sizeBytes,
                text: file.text
            )))
        }
    }
}

private extension ProductBotActions.Layout {
    func toLocal() -> Chat.LocalMessage.Content.ProductActionLayout {
        switch self {
        case .column: .column
        case .grid: .grid
        }
    }
}
