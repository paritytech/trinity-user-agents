import Foundation
import FoundationExt
import PolkadotUI
import UIKit

/// What the factory needs to draw one product body.
struct ProductBodyRenderContext {
    let messageId: String
    let status: Chat.LocalMessage.Status
    let deliveryDate: Date?
    let layoutContext: MessageLayoutContext
    let actions: ChatViewModelActions?
}

extension ChatViewModelFactory {
    func productMediaConfiguration(
        _ richText: Chat.LocalMessage.Content.ProductRichText,
        context: ProductBodyRenderContext
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let text = richText.text?.nilIfEmpty
        let view = ChatMediaStackView(
            sources: richText.media,
            text: text.map { AttributedString($0) },
            loader: productMediaLoader
        )

        var configuration = ChatMessageContainerConfiguration(
            innerContent: SwiftUIContentConfiguration(view: view),
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            canReply: false,
            contentInsets: .zero,
            layoutType: context.layoutContext.layoutType,
            identifier: ChatMediaStackView.reuseIdentifier
        )

        if let text, let actions = context.actions {
            configuration.setActions { [actions] in
                [UIAction.chatCopy { actions.copy(text) }]
            }
        }

        return .init(context.messageId, configuration)
    }

    func productFileConfiguration(
        _ file: Chat.LocalMessage.Content.ProductFile,
        context: ProductBodyRenderContext
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let url = URL(string: file.url)
        let openLink = context.actions?.openLink

        let viewModel = ChatMessageFileCell.ViewModel(
            name: file.fileName,
            preview: nil,
            size: ByteCountFormatter.string(
                fromByteCount: Int64(clamping: file.sizeBytes),
                countStyle: .file
            ),
            text: file.text?.nilIfEmpty,
            onTap: {
                guard let url else { return }
                openLink?(url)
            }
        )

        var configuration = ChatMessageContainerConfiguration.file(
            viewModel: viewModel,
            layoutType: context.layoutContext.layoutType
        )

        if let text = file.text?.nilIfEmpty, let actions = context.actions {
            configuration.setActions { [actions] in
                [UIAction.chatCopy { actions.copy(text) }]
            }
        }

        return .init(context.messageId, configuration)
    }
}
