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
    let offersDelete: Bool
}

extension ChatViewModelFactory {
    func productMediaConfiguration(
        _ richText: Chat.LocalMessage.Content.ProductRichText,
        context: ProductBodyRenderContext
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let text = richText.text?.nilIfEmpty
        let view = ChatMediaStackView(
            sources: richText.media,
            text: text.map { ChatMarkdownText(markdown: $0, onOpenLink: context.actions?.openLink) },
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

        setProductMenu(on: &configuration, text: text, context: context)

        return .init(context.messageId, configuration)
    }

    func productActionsConfiguration(
        _ productActions: Chat.LocalMessage.Content.ProductActions,
        context: ProductBodyRenderContext
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let text = productActions.text?.nilIfEmpty
        let messageId = context.messageId
        let processAction = context.actions?.processAction

        let view = ChatActionsMessageView(
            text: text.map { ChatMarkdownText(markdown: $0, onOpenLink: context.actions?.openLink) },
            buttons: productActions.buttons.map { .init(id: $0.actionId, title: $0.title) },
            layout: productActions.layout == .grid ? .grid : .column,
            onPress: { actionId in
                processAction?(.productButton(messageId: messageId, actionId: actionId))
            }
        )

        var configuration = ChatMessageContainerConfiguration(
            innerContent: SwiftUIContentConfiguration(view: view),
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            canReply: false,
            contentInsets: .zero,
            layoutType: context.layoutContext.layoutType,
            identifier: ChatActionsMessageView.reuseIdentifier
        )

        setProductMenu(on: &configuration, text: text, context: context)

        return .init(messageId, configuration)
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

        setProductMenu(on: &configuration, text: file.text?.nilIfEmpty, context: context)

        return .init(context.messageId, configuration)
    }
}

private extension ChatViewModelFactory {
    /// Copy for a body with text, then Delete when the room offers it. Delete is the last entry.
    func setProductMenu(
        on configuration: inout ChatMessageContainerConfiguration,
        text: String?,
        context: ProductBodyRenderContext
    ) {
        guard let actions = context.actions, text != nil || context.offersDelete else {
            return
        }

        let messageId = context.messageId
        let offersDelete = context.offersDelete

        configuration.setActions { [actions] in
            var elements: [UIMenuElement] = []

            if let text {
                elements.append(UIAction.chatCopy { actions.copy(text) })
            }

            if offersDelete {
                elements.append(UIAction.chatDelete { actions.delete(messageId) })
            }

            return elements
        }
    }
}
