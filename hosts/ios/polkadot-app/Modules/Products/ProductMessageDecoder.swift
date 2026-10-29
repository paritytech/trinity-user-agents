import Foundation
import PolkadotUI
import Products
import SwiftUI

final class ProductMessageDecoder: ChatMessageCustomDecoding {
    let identifier: MessageDecoderIdentifier = .product

    let resolveImage: WidgetImageResolver

    private let runtime: ChatRuntimeProtocol
    private let tokenResolver: any WidgetDesignTokenResolving
    private let description: String?
    private let logger: LoggerProtocol
    private var viewModels: [String: ProductWidgetViewModel] = [:]
    private var roomFooterCards: [String: (footer: Chat.RoomFooter, viewModel: ProductWidgetViewModel)] = [:]

    init(
        runtime: ChatRuntimeProtocol,
        tokenResolver: any WidgetDesignTokenResolving,
        description: String?,
        resolveImage: WidgetImageResolver,
        logger: LoggerProtocol
    ) {
        self.runtime = runtime
        self.tokenResolver = tokenResolver
        self.description = description
        self.resolveImage = resolveImage
        self.logger = logger
    }

    func decode(data: Data, context: ChatMessageDecodingContext) -> [any HashableContentConfiguration] {
        let viewModel = viewModels[context.messageId] ?? ProductWidgetViewModel(
            roomId: context.roomId,
            messageId: context.messageId,
            messageType: context.identifier,
            messageData: data,
            runtime: runtime,
            tokenResolver: tokenResolver,
            logger: logger
        )
        viewModels[context.messageId] = viewModel

        let messageId = context.messageId
        let processAction = context.processAction

        let widgetView = ProductWidgetChatView(
            messageId: messageId,
            nodeProvider: viewModel,
            onAction: { actionId, payload in
                processAction(.customMessage(
                    actionId: actionId,
                    payload: payload,
                    messageId: messageId
                ))
            },
            resolveImage: resolveImage
        )

        return [SwiftUIContentConfiguration(view: widgetView)]
    }

    /// The input slot that holds the card the product docked in the footer of `roomId`.
    func makeRoomFooterCard(
        roomId: String,
        messageType: String,
        payload: Data
    ) -> (any ChatInputViewConfigurationProtocol)? {
        guard let handler = runtime as? TrUAPIChatHandler else { return nil }

        let footer = Chat.RoomFooter.card(messageType: messageType, payload: payload)
        let viewModel: ProductWidgetViewModel
        if let docked = roomFooterCards[roomId], docked.footer == footer {
            viewModel = docked.viewModel
        } else {
            clearRoomFooterCard(roomId: roomId)
            viewModel = ProductWidgetViewModel(
                messageId: roomId,
                render: { await handler.renderRoomFooter(roomId: roomId, messageType: messageType, payload: payload) },
                tokenResolver: tokenResolver,
                logger: logger
            )
            roomFooterCards[roomId] = (footer, viewModel)
        }

        return ProductRoomFooterCardConfiguration(
            roomId: roomId,
            messageType: messageType,
            payload: payload,
            nodeProvider: viewModel,
            onAction: { actionId, actionPayload in
                Task {
                    await handler.dispatchRoomFooterEvent(
                        roomId: roomId,
                        messageType: messageType,
                        actionId: actionId,
                        payload: actionPayload
                    )
                }
            },
            resolveImage: resolveImage
        )
    }

    /// Stops drawing the card docked in the footer of `roomId`, once the room docks another or none.
    func clearRoomFooterCard(roomId: String) {
        roomFooterCards.removeValue(forKey: roomId)?.viewModel.renderTask?.cancel()
    }

    func previewString(data _: Data) -> String {
        description ?? String(localized: .Common.productWidgetMessage)
    }
}
