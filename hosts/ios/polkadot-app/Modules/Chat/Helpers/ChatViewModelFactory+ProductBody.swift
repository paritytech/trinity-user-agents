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

extension Chat.LocalMessage {
    /// The text that a reply quote shows for `text` of this message. A Markdown text shows without its marks.
    func quoteText(_ text: String) -> String {
        hasMarkdownText ? ChatReadAloudService.plainText(fromMarkdown: text) : text
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
        case let .notice(text): return text
        }
    }
}

// MARK: - Product text (02-DESIGN §6.1, O18)

extension ChatViewModelFactory {
    /// The format of a text in a product room: the Markdown subset for a `RichText` body with no media, else plain
    /// text with its web links. A tapped link opens through the host. Nil in a person chat, which keeps inline
    /// Markdown and the links that the system finds.
    func productText(
        of message: Chat.LocalMessage,
        text: String,
        actions: ChatViewModelActions?
    ) -> ChatMessageContainerConfiguration.ProductText? {
        guard productRoom != nil else { return nil }

        let collapse = productTextCollapse(of: message.messageId, text: text)
        return message.hasMarkdownText
            ? .markdown(openLink: actions?.openLink, collapse: collapse)
            : .plain(openLink: actions?.openLink, collapse: collapse)
    }

    /// The "Show more" button of a long product text, or nil when the user expanded the text or the text is short.
    func productTextCollapse(of messageId: String, text: String) -> ChatTextCollapse? {
        MainActor.assumeIsolated {
            guard let productRoom, text.count > ChatTextCollapse.collapsedLength else {
                return nil
            }

            let productId = productRoom.productMessageId(messageId)
            guard !productRoom.isTextExpanded(messageId) else {
                productRoom.mark("text-" + productId, "expanded")
                return nil
            }

            productRoom.mark("text-" + productId, "collapsed")
            return ChatTextCollapse(title: String(localized: .ChatExtension.messageShowMore)) { [weak productRoom] in
                MainActor.assumeIsolated { productRoom?.expandText(of: messageId) }
            }
        }
    }
}

extension ChatViewModelFactory {
    func productMediaConfiguration(
        _ richText: Chat.LocalMessage.Content.ProductRichText,
        context: ProductBodyRenderContext
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let text = richText.text?.nilIfEmpty
        let items = richText.media.indices.map { index in
            ChatMediaStackItem(
                source: richText.media[index],
                kind: Self.mediaKind(richText.info(at: index)),
                alt: richText.alt(at: index)
            )
        }
        let isOutgoing = context.status.isOutgoing
        let status = mediaStatus(context: context, hasText: text != nil)
        let view = ChatMediaStackView(
            items: items,
            text: text.map { text in
                ChatMarkdownText(
                    markdown: text,
                    onOpenLink: context.actions?.openLink,
                    collapse: productTextCollapse(of: context.messageId, text: text),
                    textColor: isOutgoing ? .fgPrimaryInverted : .fgPrimary,
                    statusPlaceholderImage: status?.placeholderImage
                )
            },
            loader: productMediaLoader,
            posterLoader: productPosterLoader,
            opening: mediaOpening(of: context.messageId)
        )

        var configuration = ChatMessageContainerConfiguration(
            innerContent: SwiftUIContentConfiguration(view: view),
            side: isOutgoing ? .trailing : .leading,
            bubbleColor: isOutgoing ? .bgSurfaceContainerInverted : .bgSurfaceContainer,
            statusConfiguration: status,
            canReply: context.canReply,
            addReaction: addReaction(context: context),
            messageReaction: messageReaction(context: context),
            contentInsets: .zero,
            statusViewInsets: Self.mediaStatusInsets,
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
        let url = URL(string: file.url).flatMap { $0.scheme?.lowercased() == "https" ? $0 : nil }
        let openLink = context.actions?.openLink
        let messageId = context.messageId
        let openFile: () -> Void = { [weak productRoom] in
            // An `https` file opens after the confirmation of the link. Inline and preimage bytes open from a
            // local file (PARITY row 4).
            if let url {
                openLink?(url)
                return
            }
            MainActor.assumeIsolated {
                guard let productRoom, let request = productRoom.mediaRequest(of: messageId, at: 0) else { return }
                Task { await productRoom.openMedia(request) }
            }
        }

        let viewModel = ChatMessageFileCell.ViewModel(
            name: file.fileName,
            preview: nil,
            size: ByteCountFormatter.string(
                fromByteCount: Int64(clamping: file.sizeBytes),
                countStyle: .file
            ),
            text: file.text?.nilIfEmpty,
            onTap: openFile
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

// MARK: - Product media (02-DESIGN §6.2, PARITY row 4)

private extension ChatViewModelFactory {
    static let mediaStatusInsets = UIEdgeInsets(top: 0, left: 0, bottom: 8, right: 8)

    /// The time and the tick of a media message, as in a person chat: over the media when there is no text, and at
    /// the end of the text when there is one.
    func mediaStatus(context: ProductBodyRenderContext, hasText: Bool) -> ChatMessageStatusViewConfiguration? {
        let background: ChatMessageStatusViewConfiguration.Background? = hasText ? nil : .mediaOverlay
        switch context.status {
        case let .outgoing(status):
            return .outbox(
                date: context.deliveryDate,
                formatter: timeFormatter,
                status: status.configurationStatus(),
                background: background
            )
        case .incoming:
            return context.deliveryDate.map { .inbox(date: $0, formatter: timeFormatter, background: background) }
        }
    }

    static func mediaKind(_ info: Chat.LocalMessage.Content.ProductMediaInfo) -> ChatMediaStackItem.Kind {
        switch info.kind {
        case .image:
            return .image
        case .video:
            return .video
        case .file:
            let size = info.sizeBytes.map {
                ByteCountFormatter.string(fromByteCount: Int64(clamping: $0), countStyle: .file)
            }
            return .file(name: info.fileName ?? String(localized: .Common.mediaFile), detail: size ?? info.mimeType)
        }
    }

    /// A tap on an item opens it through the product room. Nil outside a product room.
    func mediaOpening(of messageId: String) -> ChatMediaOpening? {
        MainActor.assumeIsolated {
            guard let productRoom else { return nil }

            let titles = ChatMediaOpening.Titles(
                retry: String(localized: .ChatExtension.mediaRetry),
                openImage: String(localized: .ChatExtension.mediaOpenImage),
                playVideo: String(localized: .ChatExtension.mediaPlayVideo),
                openFile: String(localized: .ChatExtension.mediaOpenFile),
                failed: String(localized: .ChatExtension.mediaFailed)
            )
            return ChatMediaOpening(titles: titles) { [weak productRoom] index in
                guard let productRoom, let request = productRoom.mediaRequest(of: messageId, at: index) else {
                    return false
                }
                return await productRoom.openMedia(request)
            }
        }
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
    ///
    /// A failed attachment of the user is an attachment that the host could not stage or send (P10.3). It offers
    /// Retry and Delete in every room, and no Copy: its text tells only its state.
    func setProductMenu(
        on configuration: inout ChatMessageContainerConfiguration,
        text: String?,
        context: ProductBodyRenderContext
    ) {
        let asksProduct = MainActor.assumeIsolated { productRoom?.asksMessageMenus == true }
        let failed = context.status == .outgoing(.failed)
        guard
            let actions = context.actions,
            context.canReply || text != nil || context.offersDelete || asksProduct || failed else {
            return
        }

        var hostActions: [ChatMessageBubbleAction] = []
        if context.canReply, !failed {
            hostActions.append(.reply)
        }
        if text != nil, !failed {
            hostActions.append(.copy)
        }
        if failed {
            hostActions.append(.retry)
        }
        if context.offersDelete || failed {
            hostActions.append(.delete)
        }

        configuration.setActions(makeMessageMenu(
            messageId: context.messageId,
            hostActions: hostActions,
            content: ChatMessageMenuContent(text: text ?? "", actions: actions)
        ))
    }
}
