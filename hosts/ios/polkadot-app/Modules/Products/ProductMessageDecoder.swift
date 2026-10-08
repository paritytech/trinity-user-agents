import Foundation
import PolkadotUI
import Products
import SwiftUI

final class ProductMessageDecoder: ChatMessageCustomDecoding {
    let identifier: MessageDecoderIdentifier = .product

    private let runtime: ChatRuntimeProtocol
    private let tokenResolver: any WidgetDesignTokenResolving
    private let resolveImage: WidgetImageResolver?
    private let logger: LoggerProtocol
    private var viewModels: [String: ProductWidgetViewModel] = [:]

    init(
        runtime: ChatRuntimeProtocol,
        tokenResolver: any WidgetDesignTokenResolving,
        resolveImage: WidgetImageResolver?,
        logger: LoggerProtocol
    ) {
        self.runtime = runtime
        self.tokenResolver = tokenResolver
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

    func previewString(data _: Data) -> String {
        String(localized: .Common.productWidgetMessage)
    }
}
