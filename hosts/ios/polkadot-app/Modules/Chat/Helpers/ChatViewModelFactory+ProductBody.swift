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
    let canReply: Bool
    let reactions: [ReactionViewModel]
    /// Nil means the room offers no reactions.
    let reactionChoice: ChatReactionChoice?
}

extension Chat.LocalMessage.Content {
    /// The line that a reply quotes from a product body, or nil for other content.
    var productReplyPreview: String? {
        guard case let .productBody(body) = self else {
            return nil
        }

        return body.replyPreview
    }
}

extension Chat.LocalMessage.Content.ProductBody {
    /// The line that a reply quotes from the body.
    var replyPreview: String {
        if let text = text?.nilIfEmpty {
            return text
        }

        switch self {
        case .richText: return String(localized: .Common.mediaFile)
        case .actions: return String(localized: .ChatExtension.messagePreviewActions)
        case let .file(file): return file.fileName
        }
    }
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
            canReply: context.canReply,
            addReaction: addReaction(context: context),
            messageReaction: messageReaction(context: context),
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
            canReply: context.canReply,
            addReaction: addReaction(context: context),
            messageReaction: messageReaction(context: context),
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
            layoutType: context.layoutContext.layoutType,
            canReply: context.canReply,
            addReaction: addReaction(context: context),
            messageReaction: messageReaction(context: context)
        )

        setProductMenu(on: &configuration, text: file.text?.nilIfEmpty, context: context)

        return .init(context.messageId, configuration)
    }
}

private extension ChatViewModelFactory {
    func addReaction(context: ProductBodyRenderContext) -> ChatMessageContainerConfiguration.AddReactionViewModel? {
        context.canReply
            ? addReactionViewModel(
                messageId: context.messageId,
                choice: context.reactionChoice,
                actions: context.actions
            )
            : nil
    }

    func messageReaction(
        context: ProductBodyRenderContext
    ) -> ChatMessageContainerConfiguration.MessageReactionViewModel? {
        messageReactionViewModel(messageId: context.messageId, reactions: context.reactions, actions: context.actions)
    }

    /// Reply when the room has an input, Copy for a body with text, then Delete when the room offers it.
    /// Delete is the last host entry. A room that asks its product adds the product entries (P6.1).
    func setProductMenu(
        on configuration: inout ChatMessageContainerConfiguration,
        text: String?,
        context: ProductBodyRenderContext
    ) {
        let asksProduct = MainActor.assumeIsolated { productRoom?.asksMessageMenus == true }
        guard
            let actions = context.actions,
            context.canReply || text != nil || context.offersDelete || asksProduct else {
            return
        }

        var hostActions: [ChatMessageBubbleAction] = []
        if context.canReply {
            hostActions.append(.reply)
        }
        if text != nil {
            hostActions.append(.copy)
        }
        if context.offersDelete {
            hostActions.append(.delete)
        }

        configuration.setActions(makeMessageMenu(
            messageId: context.messageId,
            hostActions: hostActions,
            content: ChatMessageMenuContent(text: text ?? "", actions: actions)
        ))
    }
}
