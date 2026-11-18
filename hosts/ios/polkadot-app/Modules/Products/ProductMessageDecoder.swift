import Foundation
import PolkadotUI
import Products
import SwiftUI

final class ProductMessageDecoder: ChatMessageCustomDecoding {
    let identifier: MessageDecoderIdentifier = .product

    let resolveImage: WidgetImageResolver

    private let runtime: ChatRuntimeProtocol
    private let webFooterContent: ProductRoomFooterWebContent?
    private let tokenResolver: any WidgetDesignTokenResolving
    private let description: String?
    private let logger: LoggerProtocol
    private var viewModels: [String: ProductWidgetViewModel] = [:]
    private var roomFooterCards: [String: (footer: Chat.RoomFooter, viewModel: ProductWidgetViewModel)] = [:]
    private var refusedRoomFooters: [String: Chat.RoomFooter] = [:]

    init(
        runtime: ChatRuntimeProtocol,
        webFooterContent: ProductRoomFooterWebContent? = nil,
        tokenResolver: any WidgetDesignTokenResolving,
        description: String?,
        resolveImage: WidgetImageResolver,
        logger: LoggerProtocol
    ) {
        self.runtime = runtime
        self.webFooterContent = webFooterContent
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
        let footer = Chat.RoomFooter.card(messageType: messageType, payload: payload)
        guard let handler = runtime as? TrUAPIChatHandler else {
            if refusedRoomFooters[roomId] != footer {
                refusedRoomFooters[roomId] = footer
                logger
                    .warning(
                        "[truapi.product] A \(messageType) card is docked, but only the TrUAPI chat runtime can draw one"
                    )
            }
            return nil
        }

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

    /// The input slot that shows the page of the product's own web content docked in the footer of `roomId`.
    ///
    /// A press the page reports reaches the product as a press in the room footer with an empty
    /// message type, the way a button on a docked card does.
    func makeRoomFooterWeb(
        roomId: String,
        path: String,
        height: UInt32
    ) -> (any ChatInputViewConfigurationProtocol)? {
        // The room docks a page now, so the card it held stops rendering.
        roomFooterCards.removeValue(forKey: roomId)?.viewModel.renderTask?.cancel()
        let footer = Chat.RoomFooter.web(path: path, height: height)
        guard let handler = runtime as? TrUAPIChatHandler,
              let content = webFooterContent,
              let url = ProductRoomFooterWebPage.url(path: path, productId: content.productId) else {
            if refusedRoomFooters[roomId] != footer {
                refusedRoomFooters[roomId] = footer
                logger.warning("[truapi.product] A web footer is docked at \(path), but this host cannot show it")
            }
            return nil
        }

        let logger = logger
        return ProductRoomFooterWebConfiguration(
            roomId: roomId,
            path: path,
            height: height,
            makeView: {
                ProductRoomFooterWebView(
                    pageURL: url,
                    productId: content.productId,
                    height: height,
                    schemeHandler: content.makeSchemeHandler(),
                    onAction: { actionId, payload in
                        await handler.dispatchRoomFooterEvent(
                            roomId: roomId,
                            messageType: "",
                            actionId: actionId,
                            payload: payload
                        )
                    },
                    logger: logger
                )
            }
        )
    }

    /// Stops drawing the card docked in the footer of `roomId`, once the room docks another or none.
    func clearRoomFooterCard(roomId: String) {
        refusedRoomFooters.removeValue(forKey: roomId)
        roomFooterCards.removeValue(forKey: roomId)?.viewModel.renderTask?.cancel()
    }

    func previewString(data _: Data) -> String {
        description ?? String(localized: .Common.productWidgetMessage)
    }
}
