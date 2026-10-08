import Foundation
import PolkadotUI
import Products
import SwiftUI

final class ProductMessageDecoder: ChatMessageCustomDecoding {
    let identifier: MessageDecoderIdentifier = .product

    private let runtime: ChatRuntimeProtocol
    private let tokenResolver: any WidgetDesignTokenResolving
    private let description: String?
    private let logger: LoggerProtocol
    private var viewModels: [String: ProductWidgetViewModel] = [:]

    init(
        runtime: ChatRuntimeProtocol,
        tokenResolver: any WidgetDesignTokenResolving,
        description: String?,
        logger: LoggerProtocol
    ) {
        self.runtime = runtime
        self.tokenResolver = tokenResolver
        self.description = description
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
            nodeProvider: viewModel
        ) { actionId, payload in
            processAction(.customMessage(
                actionId: actionId,
                payload: payload,
                messageId: messageId
            ))
        }

        return [SwiftUIContentConfiguration(view: widgetView)]
    }

    func previewString(data _: Data) -> String {
        description ?? String(localized: .Common.productWidgetMessage)
    }
}
