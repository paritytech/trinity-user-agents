// swiftlint:disable file_length
import ExternalAccessibility
import Foundation
import PolkadotUI
import Foundation_iOS
import UIKit.UIImage
import Operation_iOS
import Products

protocol ChatViewModelMaking {
    func viewModel(
        model: MessageListModel,
        metadata: MessageListMetadata,
        actions: ChatViewModelActions,
        footerConfiguration: (any HashableContentConfiguration)?
    ) -> ChatViewLayout.ViewModel

    func transferPreviewText(
        content: Chat.LocalMessage.Content.Transfer,
        isIncoming: Bool,
        peerName: String
    ) -> String

    func isMessageVisible(_ message: Chat.LocalMessage) -> Bool
}

final class ChatViewModelFactory {
    private static let replyPreviewMaxLength = 50

    let initialMessageId = UUID().uuidString
    let newMessagesId = UUID().uuidString

    let timeFormatter: TimestampFormatting
    let balanceFactory: TransferAmountViewModelFactoryProtocol
    let dateSeparatorFormatter: ChatDateSeparatorFormatter
    let layoutContextCalculator: MessageLayoutContextCalculator

    let customDecodersById: [UInt8: ChatMessageCustomDecoding]
    let attachmentViewModelFactory: ChatAttachmentViewModelMaking
    let productRepository: AnyDataProviderRepository<Product>
    let productNameCache: ProductNameCaching
    let flowState: SPAFlowState
    let logger: LoggerProtocol

    init(
        timeFormatter: TimestampFormatting = MessageTimestampFormatter(),
        balanceFactory: TransferAmountViewModelFactoryProtocol,
        dateSeparatorFormatter: ChatDateSeparatorFormatter = ChatDateSeparatorFormatter(),
        layoutContextCalculator: MessageLayoutContextCalculator = MessageLayoutContextCalculator(),
        customDecoders: [ChatMessageCustomDecoding],
        attachmentViewModelFactory: ChatAttachmentViewModelMaking,
        productRepository: AnyDataProviderRepository<Product>,
        productNameCache: ProductNameCaching,
        flowState: SPAFlowState,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.timeFormatter = timeFormatter
        self.balanceFactory = balanceFactory
        self.dateSeparatorFormatter = dateSeparatorFormatter
        self.layoutContextCalculator = layoutContextCalculator
        self.attachmentViewModelFactory = attachmentViewModelFactory
        self.productRepository = productRepository
        self.productNameCache = productNameCache
        self.flowState = flowState
        self.logger = logger
        customDecodersById = customDecoders.reduce(into: [:]) {
            $0[$1.identifier.rawValue] = $1
        }
    }
}

extension ChatViewModelFactory: ChatViewModelMaking {
    func viewModel(
        model: MessageListModel,
        metadata: MessageListMetadata,
        actions: ChatViewModelActions,
        footerConfiguration: (any HashableContentConfiguration)?
    ) -> ChatViewLayout.ViewModel {
        let headerConfiguration = metadata.chatMetadata.chatContactInfo

        let inputConfiguration = makeInputConfiguration(for: metadata.chatMetadata, actions: actions)

        let scrollDownConfiguration = ChatViewLayout.ViewModel.ScrollDownButtonConfiguration(
            available: true,
            unreadCount: model.newMessageCount,
        )

        let scrollToReactionConfiguration = ChatViewLayout.ViewModel.ScrollToReactionButtonConfiguration(
            targetMessageId: model.oldestNewReactionTargetMessageId
        )

        let sections = makeSections(model: model, metadata: metadata, actions: actions)

        let initiallyVisibleMessageIdentifier = model.initiallyUnreadMessage ??
            sections.last?.messages.last?.id

        return ChatViewLayout.ViewModel(
            headerConfiguration: headerConfiguration,
            chatInputConfiguration: inputConfiguration,
            initiallyVisibleMessageIdentifier: initiallyVisibleMessageIdentifier,
            firstUnreadMessageIdentifier: model.firstUnreadMessageId,
            scrollDownConfiguration: scrollDownConfiguration,
            scrollToReactionConfiguration: scrollToReactionConfiguration,
            sections: sections,
            footerConfiguration: footerConfiguration
        )
    }

    func transferPreviewText(
        content: Chat.LocalMessage.Content.Transfer,
        isIncoming: Bool,
        peerName: String
    ) -> String {
        let amountString = balanceFactory.amount(from: content.totalValue)

        if isIncoming {
            return String(localized: .chatTransferReceived(contact: peerName, amount: amountString))
        } else {
            return String(localized: .chatTransferSent(amount: amountString))
        }
    }

    func isMessageVisible(_ message: Chat.LocalMessage) -> Bool {
        // Should be in sync with convert(message:listModel:metadata:)
        switch message.content {
        case .token,
             .reacted,
             .reactionRemoved,
             .edited:
            return false

        case .call(.offer):
            return true

        case .call:
            return false

        case let .chatRequest(content):
            if content.welcomeMessage?.text != nil {
                return true
            } else {
                // Incoming chat requests without a welcome message text are invisible in the message list
                return message.status.isOutgoing
            }

        case let .versionedChatRequest(versionedContent):
            if versionedContent.ensureV1().welcomeMessage?.text != nil {
                return true
            } else {
                return message.status.isOutgoing
            }

        case let .customRendered(content):
            // Visible only if a decoder is registered and successfully produces a configuration
            guard let decoder = customDecodersById[content.decoderId] else {
                return false
            }

            let decodingContext = ChatMessageDecodingContext(
                messageId: message.messageId,
                roomId: message.chatId.roomId,
                identifier: content.identifier,
                processAction: { _ in }
            )

            return !decoder.decode(data: content.data, context: decodingContext).isEmpty

        default:
            return true
        }
    }
}

private extension ChatViewModelFactory {
    func convertToReactionViewModels(_ aggregates: [Chat.MessageReactionAggregate]) -> [ReactionViewModel] {
        aggregates.map { aggregate in
            ReactionViewModel(
                emoji: aggregate.emoji,
                count: aggregate.count,
                isSelectedByCurrentUser: aggregate.reactedByCurrentUser
            )
        }
    }

    func makeSections(
        model: MessageListModel,
        metadata: MessageListMetadata,
        actions: ChatViewModelActions
    ) -> [ChatViewLayout.Section] {
        model.orderedSections.compactMap { section in
            let messageConfiguration = makeMessageConfigurations(
                model: model,
                section: section,
                metadata: metadata,
                actions: actions
            )

            guard !messageConfiguration.isEmpty else {
                return nil
            }

            let dateText = makeDateText(section: section)

            return .init(
                identifier: dateText,
                dateText: dateText,
                messages: messageConfiguration
            )
        }
    }

    func makeMessageConfigurations(
        model: MessageListModel,
        section: MessageListSection,
        metadata: MessageListMetadata,
        actions: ChatViewModelActions
    ) -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        let messages = model.messagesBySection[section] ?? []
        var result: [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] = []

        let layoutContexts = calculateLayoutContexts(for: messages)

        for message in messages {
            if message.identifier == model.initiallyUnreadMessage {
                result.append(newMessagesConfiguration())
            }

            let reactions = model.reactionsByMessageId[message.messageId].map { convertToReactionViewModels($0) } ?? []
            let layoutContext = layoutContexts[message.messageId] ?? .standard

            result.append(
                contentsOf: convert(
                    message,
                    listModel: model,
                    metadata: metadata,
                    actions: actions,
                    reactions: reactions,
                    layoutContext: layoutContext
                )
            )
        }

        return result
    }

    func calculateLayoutContexts(for messages: [Chat.LocalMessage]) -> [String: MessageLayoutContext] {
        layoutContextCalculator.calculateLayoutContexts(for: messages)
    }

    func makeDateText(section: MessageListSection) -> String {
        switch section {
        case .today:
            dateSeparatorFormatter.string(for: Date())
        case .yesterday:
            dateSeparatorFormatter.string(for: Calendar.current.date(byAdding: .day, value: -1, to: Date())!)
        case let .other(date):
            dateSeparatorFormatter.string(for: date)
        }
    }

    // MARK: message.content mapping

    // swiftlint:disable:next function_body_length cyclomatic_complexity
    func convert(
        _ message: Chat.LocalMessage,
        listModel: MessageListModel,
        metadata: MessageListMetadata,
        actions: ChatViewModelActions,
        reactions: [ReactionViewModel],
        layoutContext: MessageLayoutContext
    ) -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        let status = message.status
        let latestEdit = listModel.getLatestEdit(for: message.messageId)
        let isEdited = listModel.isEdited(message.messageId)

        let deliveryDate: Date? = (layoutContext.showTimestamp || isEdited)
            ? Date.fromChatTimestamp(message.timestamp)
            : nil

        let inboxBubbleStyle = PolkadotPrizesBubbleStyleResolver.style(
            for: metadata.chatMetadata.chatId
        )

        switch message.content {
        case let .text(text),
             let .extensionActionResponse(text, _):
            return [
                textMessageConfiguration(
                    messageId: message.messageId,
                    text: latestEdit?.newText ?? text,
                    messageActions: ChatMessageBubbleAction.textMessageActions(
                        for: status,
                        input: metadata.peerMetadata.input,
                        isEdited: isEdited
                    ),
                    status: status,
                    deliveryDate: deliveryDate,
                    layoutContext: layoutContext,
                    reactions: reactions,
                    actions: actions,
                    inboxBubbleStyle: inboxBubbleStyle
                )
            ]

        case let .richText(richText):
            return [
                textMessageConfiguration(
                    messageId: message.messageId,
                    text: latestEdit?.newText ?? richText.text ?? "",
                    messageActions: ChatMessageBubbleAction.textMessageActions(
                        for: status,
                        input: metadata.peerMetadata.input,
                        isEdited: isEdited
                    ),
                    status: status,
                    deliveryDate: deliveryDate,
                    layoutContext: layoutContext,
                    attachments: richText.attachments,
                    reactions: reactions,
                    actions: actions,
                    inboxBubbleStyle: inboxBubbleStyle
                )
            ]

        case let .customRendered(content):
            let decodingContext = ChatMessageDecodingContext(
                messageId: message.messageId,
                roomId: message.chatId.roomId,
                identifier: content.identifier,
                processAction: actions.processAction
            )
            guard let configurations = customDecodersById[content.decoderId]?
                .decode(data: content.data, context: decodingContext) else {
                return []
            }
            return configurations.enumerated().map { index, config in
                // allow to read message by message id
                if index == 0 {
                    .init(message.messageId, config)
                } else {
                    .init("\(message.messageId)-\(index)", config)
                }
            }

        case .token,
             .reacted,
             .reactionRemoved,
             .edited,
             .deviceAdded,
             .deviceRemoved:
            // system, should be ignored
            return []

        case let .send(content):
            let claimContent = Chat.LocalMessage.Content.Transfer(
                totalValue: content.amount,
                coinKeys: []
            )
            return [
                transferMessageConfiguration(
                    messageId: message.messageId,
                    peerMetadata: metadata.peerMetadata,
                    content: claimContent,
                    status: message.status,
                    deliveryDate: deliveryDate,
                    reactions: reactions,
                    actions: actions
                )
            ]

        case .contactAdded:
            return [
                contactAdded(
                    messageId: message.messageId,
                    peerMetadata: metadata.peerMetadata,
                    status: message.status
                )
            ]

        case .leftChat:
            return [
                leftChat(
                    messageId: message.messageId,
                    peerMetadata: metadata.peerMetadata,
                    status: message.status
                )
            ]

        case let .reply(replyContent):
            let replyInfo = makeReplyInfo(
                replyContent: replyContent,
                listModel: listModel,
                peerMetadata: metadata.peerMetadata
            )

            let replyDisplayText = latestEdit?.newText ?? replyContent.ownContent.text ?? ""
            return [
                textMessageConfiguration(
                    messageId: message.messageId,
                    text: replyDisplayText,
                    messageActions: ChatMessageBubbleAction.textMessageActions(
                        for: status,
                        input: metadata.peerMetadata.input,
                        isEdited: isEdited
                    ),
                    status: status,
                    deliveryDate: deliveryDate,
                    layoutContext: layoutContext,
                    replyInfo: replyInfo,
                    reactions: reactions,
                    actions: actions,
                    inboxBubbleStyle: inboxBubbleStyle
                )
            ]

        case let .chatRequest(content):
            return chatRequestConfigurations(
                content: content,
                message: message,
                metadata: metadata,
                status: status,
                deliveryDate: deliveryDate,
                layoutContext: layoutContext,
                reactions: reactions,
                actions: actions,
                inboxBubbleStyle: inboxBubbleStyle
            )

        case let .versionedChatRequest(versionedContent):
            return chatRequestConfigurations(
                content: versionedContent.ensureV1(),
                message: message,
                metadata: metadata,
                status: status,
                deliveryDate: deliveryDate,
                layoutContext: layoutContext,
                reactions: reactions,
                actions: actions,
                inboxBubbleStyle: inboxBubbleStyle
            )

        case .chatAccepted,
             .multiChatAccepted:
            return [
                chatAccepted(
                    messageId: message.messageId,
                    peerMetadata: metadata.peerMetadata,
                    status: status
                )
            ]

        case .unsupported:
            return [
                textMessageConfiguration(
                    messageId: message.messageId,
                    text: String(localized: .chatUnsupportedContent),
                    messageActions: [],
                    status: status,
                    deliveryDate: deliveryDate,
                    layoutContext: layoutContext,
                    reactions: reactions,
                    actions: nil,
                    inboxBubbleStyle: inboxBubbleStyle
                )
            ]

        case let .call(payload):
            return callConfigurations(
                for: payload,
                message: message,
                status: status,
                deliveryDate: deliveryDate,
                actions: actions
            )

        case let .staticTextImageContent(content):
            return [
                textImageMessageConfiguration(
                    messageId: message.messageId,
                    text: content.text,
                    image: content.media,
                    layoutContext: layoutContext,
                    actions: actions,
                    inboxBubbleStyle: inboxBubbleStyle
                )
            ]

        case let .file(file):
            return [
                fileMessageConfiguration(
                    file,
                    messageId: message.messageId,
                    actions: actions,
                    layoutContext: layoutContext
                )
            ]

        case let .coinageSend(content):
            return [
                transferMessageConfiguration(
                    messageId: message.messageId,
                    peerMetadata: metadata.peerMetadata,
                    content: content,
                    status: message.status,
                    deliveryDate: deliveryDate,
                    reactions: reactions,
                    actions: actions
                )
            ]

        case .compactedMessages:
            return []
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    private func makeReplyInfo(
        replyContent: Chat.RemoteMessageContentV1.MessageContent.ReplyContent,
        listModel: MessageListModel,
        peerMetadata: Chat.PeerMetadata
    ) -> ChatMessageContainerConfiguration.ReplyInfo? {
        guard let originalMessage = listModel.getMessage(by: replyContent.messageId) else {
            return nil
        }

        let originalText: String

        if let originalEditedText = listModel.getLatestEdit(for: replyContent.messageId)?.newText {
            originalText = originalEditedText
        } else {
            switch originalMessage.content {
            case let .text(text):
                originalText = text
            case let .richText(richText):
                let displayText = richText.text ?? ""
                originalText = !displayText.isEmpty ? displayText : String(localized: .Common.mediaFile)
            case let .reply(reply):
                originalText = reply.ownContent.text ?? ""
            case let .send(content):
                // Legacy
                originalText = transferPreviewText(
                    content: .init(totalValue: content.amount, coinKeys: []),
                    isIncoming: originalMessage.status.isIncoming,
                    peerName: peerMetadata.name
                )
            case let .coinageSend(content):
                originalText = transferPreviewText(
                    content: content,
                    isIncoming: originalMessage.status.isIncoming,
                    peerName: peerMetadata.name
                )
            case let .chatRequest(content):
                originalText = content.welcomeMessage?.text ?? ""
            case let .versionedChatRequest(content):
                originalText = content.ensureV1().welcomeMessage?.text ?? ""
            default:
                return nil
            }
        }

        let username: String =
            switch originalMessage.status {
            case .incoming:
                peerMetadata.name
            case .outgoing:
                String(localized: .chatReplyYou)
            }

        return ChatMessageContainerConfiguration.ReplyInfo(
            username: username,
            preview: String(originalText.prefix(Self.replyPreviewMaxLength)),
            messageId: replyContent.messageId
        )
    }

    func newMessagesConfiguration() -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let configuration = ChatInfoMessageConfiguration.newMessages()
        return .init(id: newMessagesId, configuration: configuration)
    }

    func textMessageConfiguration(
        messageId: String,
        text: String,
        messageActions: [ChatMessageBubbleAction],
        status: Chat.LocalMessage.Status,
        deliveryDate: Date?,
        layoutContext: MessageLayoutContext = .standard,
        replyInfo: ChatMessageContainerConfiguration.ReplyInfo? = nil,
        attachments: [Chat.LocalMessage.Content.Attachment]? = nil,
        reactions: [ReactionViewModel] = [],
        actions: ChatViewModelActions? = nil,
        inboxBubbleStyle: InboxBubbleStyle? = nil
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let (textAfterStrip, productLinkPreview) = resolveProductLinkPreview(
            text: text,
            status: status,
            actions: actions
        )

        let resolvedText: String
        if textAfterStrip.isEmpty,
           attachments?.isEmpty ?? true,
           productLinkPreview == nil {
            logger.warning("Empty content detected for message \(messageId), rendering placeholder")
            resolvedText = "⊘"
        } else {
            resolvedText = textAfterStrip
        }

        let isEdited = messageActions.contains { $0 == .edited }
        let canReact = messageActions.contains(.reaction)

        let attachmentViewModels: [ChatRichTextMessageConfiguration.AttachmentItem]? =
            if let attachments {
                attachments.compactMap { attachment in
                    attachmentViewModelFactory.makeAttachmentItem(
                        for: attachment,
                        messageId: messageId,
                        onSelection: {
                            actions?.selectAttachment(attachment)
                        }
                    )
                }
            } else {
                nil
            }

        let addReaction: ChatMessageContainerConfiguration.AddReactionViewModel? = canReact
            ? .init(
                quickEmojis: Chat.quickReactions,
                allSections: Chat.allReactions.map { EmojiPickerInline.Section(title: $0.category, emojis: $0.emojis) },
                onReactionTap: actions
                    .map { action in
                        { [messageId] emoji in action.toggleReaction(messageId, emoji) }
                    }
            )
            : nil

        let messageReaction: ChatMessageContainerConfiguration.MessageReactionViewModel? = reactions.isEmpty
            ? nil
            : .init(
                reactions: reactions,
                onReactionTap: actions.map { action in { [messageId] emoji in
                    action.toggleReaction(messageId, emoji)
                }
                },
                onReactionLongPress: actions.map { action in { [messageId] in
                    action.showReactionDetails(messageId)
                }
                }
            )

        let context = TextMessageContext(
            text: resolvedText,
            attachments: attachmentViewModels,
            productLinkPreview: productLinkPreview,
            status: status,
            deliveryDate: deliveryDate,
            layoutContext: layoutContext,
            replyInfo: replyInfo,
            addReaction: addReaction,
            messageReaction: messageReaction,
            isEdited: isEdited,
            canReply: messageActions.contains(.reply),
            inboxBubbleStyle: inboxBubbleStyle
        )

        var configuration = makeTextMessageContainerConfiguration(context: context)

        guard let actions else {
            return .init(messageId, configuration)
        }

        let menuActions = makeTextMessageMenuActions(
            text: resolvedText,
            messageId: messageId,
            messageActions: messageActions,
            actions: actions
        )
        configuration.setActions(menuActions)

        return .init(messageId, configuration)
    }

    private func makeTextMessageContainerConfiguration(
        context: TextMessageContext
    ) -> ChatMessageContainerConfiguration {
        TextMessageConfigurationFactory.make(context: context, timeFormatter: timeFormatter)
    }

    private func makeTextMessageMenuActions(
        text: String,
        messageId: String,
        messageActions: [ChatMessageBubbleAction],
        actions: ChatViewModelActions
    ) -> () -> [UIMenuElement] {
        { [text, messageId, actions] in
            messageActions.compactMap { action in
                switch action {
                case .reply: UIAction.chatReply { actions.reply(messageId) }
                case .edit: UIAction.chatEdit { actions.edit(messageId) }
                case .copy: UIAction.chatCopy { actions.copy(text) }
                case .reaction: nil
                case .edited: UIAction.chatEditHistory { actions.showEditHistory(messageId) }
                }
            }
        }
    }

    // swiftlint:disable:next function_parameter_count
    func transferMessageConfiguration(
        messageId: String,
        peerMetadata: Chat.PeerMetadata,
        content: Chat.LocalMessage.Content.Transfer,
        status: Chat.LocalMessage.Status,
        deliveryDate: Date?,
        reactions: [ReactionViewModel],
        actions: ChatViewModelActions
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        // TODO: fetch info by content.assetId + verify on real transfer
        let projection = content.bubbleProjection
        let amountString = balanceFactory.amount(from: projection.displayedValue)
        let tokenSymbol = balanceFactory.symbol
        let originalAmountString: String? = projection.originalValue.flatMap {
            balanceFactory.amount(from: $0)
        }

        let statusConfiguration: ChatMessageStatusViewConfiguration
        var configuration: ChatMessageContainerConfiguration

        let allSections = Chat.allReactions.map { EmojiPickerInline.Section(title: $0.category, emojis: $0.emojis) }
        let addReaction = ChatMessageContainerConfiguration.AddReactionViewModel(
            quickEmojis: Chat.quickReactions,
            allSections: allSections,
            onReactionTap: { [messageId, actions] emoji in
                actions.toggleReaction(messageId, emoji)
            }
        )
        let messageReaction: ChatMessageContainerConfiguration.MessageReactionViewModel? = reactions.isEmpty
            ? nil
            : .init(
                reactions: reactions,
                onReactionTap: { [messageId, actions] emoji in
                    actions.toggleReaction(messageId, emoji)
                },
                onReactionLongPress: { [messageId, actions] in
                    actions.showReactionDetails(messageId)
                }
            )

        switch status {
        case .incoming:
            statusConfiguration = .inbox(date: deliveryDate, formatter: timeFormatter)

            configuration = ChatTransferMessageConfiguration.inbox(
                currencySymbol: AppConfig.Brand.fiatSymbol,
                amount: amountString,
                tokenSymbol: tokenSymbol,
                originalAmount: originalAmountString,
                from: peerMetadata.name,
                state: content.incomingViewState,
                statusConfiguration: statusConfiguration,
                addReaction: addReaction,
                messageReaction: messageReaction
            )

        case let .outgoing(outgoingStatus):
            statusConfiguration = .outbox(
                date: deliveryDate,
                formatter: timeFormatter,
                status: outgoingStatus.configurationStatus()
            )

            configuration = ChatTransferMessageConfiguration.outbox(
                currencySymbol: AppConfig.Brand.fiatSymbol,
                amount: amountString,
                tokenSymbol: tokenSymbol,
                originalAmount: originalAmountString,
                state: content.outgoingViewState,
                statusConfiguration: statusConfiguration,
                addReaction: addReaction,
                messageReaction: messageReaction
            )
        }

        let menuActions: () -> [UIMenuElement] = { [actions] in
            let reply = UIAction.chatReply {
                actions.reply(messageId)
            }

            return [reply]
        }
        configuration.setActions(menuActions)

        return .init(messageId, configuration)
    }

    func chatAccepted(
        messageId: String,
        peerMetadata: Chat.PeerMetadata,
        status: Chat.LocalMessage.Status
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        switch status {
        case .incoming:
            let config = ChatInfoMessageConfiguration.chatRequestAccepted(by: peerMetadata.name)
            return .init(id: messageId, configuration: config)
        case .outgoing:
            let config = ChatInfoMessageConfiguration.chatRequestAcceptedByYou()
            return .init(id: messageId, configuration: config)
        }
    }

    func contactAdded(
        messageId: String,
        peerMetadata: Chat.PeerMetadata,
        status: Chat.LocalMessage.Status
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        switch status {
        case .incoming:
            let configuration = ChatInfoMessageConfiguration.youAdded(by: peerMetadata.name)
            return .init(id: messageId, configuration: configuration)
        case .outgoing:
            let configuration = ChatInfoMessageConfiguration.youAdded(username: peerMetadata.name)
            return .init(id: messageId, configuration: configuration)
        }
    }

    func leftChat(
        messageId: String,
        peerMetadata: Chat.PeerMetadata,
        status: Chat.LocalMessage.Status
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        switch status {
        case .incoming:
            let configuration = ChatInfoMessageConfiguration.leftChat(username: peerMetadata.name)
            return .init(id: messageId, configuration: configuration)
        case .outgoing:
            let configuration = ChatInfoMessageConfiguration.youLeft()
            return .init(id: messageId, configuration: configuration)
        }
    }

    // swiftlint:disable:next function_parameter_count
    func chatRequestConfigurations(
        content: Chat.RequestContentV1,
        message: Chat.LocalMessage,
        metadata: MessageListMetadata,
        status: Chat.LocalMessage.Status,
        deliveryDate: Date?,
        layoutContext: MessageLayoutContext,
        reactions: [ReactionViewModel],
        actions: ChatViewModelActions,
        inboxBubbleStyle: InboxBubbleStyle?
    ) -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        let info = ChatInfoMessageConfiguration.chatRequested()
        if let text = content.welcomeMessage?.text {
            let isAccepted = metadata.chatMetadata.state == .created
            let messageActions: [ChatMessageBubbleAction] = isAccepted
                ? ChatMessageBubbleAction.textMessageActions(
                    for: status,
                    input: metadata.peerMetadata.input,
                    isEdited: false
                )
                .filter { $0 != .edit } : [.copy]
            let textConfig = textMessageConfiguration(
                messageId: message.messageId,
                text: text,
                messageActions: messageActions,
                status: status,
                deliveryDate: deliveryDate,
                layoutContext: layoutContext,
                reactions: reactions,
                actions: actions,
                inboxBubbleStyle: inboxBubbleStyle
            )

            if status.isOutgoing {
                return [
                    IdentifiableAnyContentConfiguration("\(message.messageId)-header", info),
                    textConfig
                ]
            } else {
                return [textConfig]
            }
        } else {
            if status.isOutgoing {
                return [
                    IdentifiableAnyContentConfiguration(message.messageId, info)
                ]
            } else {
                return []
            }
        }
    }
}

private extension ChatViewModelFactory {
    func textImageMessageConfiguration(
        messageId: String,
        text: String?,
        image: UIImage,
        layoutContext: MessageLayoutContext = .standard,
        actions: ChatViewModelActions,
        inboxBubbleStyle: InboxBubbleStyle? = nil
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let accessibilityId = AccessibilityID.Game.welcomeCard(text: text)
        var cellConfig: ChatMessageContainerConfiguration =
            if let style = inboxBubbleStyle {
                .botTextImage(
                    text: text,
                    image: image,
                    bubbleColor: style.bubbleColor,
                    bubbleStrokeColor: style.strokeColor,
                    bubbleStrokeWidth: style.strokeWidth,
                    lockFullWidth: true,
                    layoutType: layoutContext.layoutType,
                    accessibilityId: accessibilityId
                )
            } else {
                .botTextImage(
                    text: text,
                    image: image,
                    layoutType: layoutContext.layoutType,
                    accessibilityId: accessibilityId
                )
            }

        guard let text else {
            return .init(messageId, cellConfig)
        }

        let menuActions: () -> [UIMenuElement] = { [text, actions] in
            let copy = UIAction.chatCopy {
                actions.copy(text)
            }

            return [copy]
        }
        cellConfig.setActions(menuActions)
        return .init(messageId, cellConfig)
    }

    func fileMessageConfiguration(
        _ file: Chat.LocalMessage.Content.File,
        messageId: String,
        actions: ChatViewModelActions?,
        layoutContext: MessageLayoutContext = .standard
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let url = file.url
        let resourceValues = try? url.resourceValues(forKeys: [.fileSizeKey, .totalFileSizeKey])
        let fileSize = resourceValues?.totalFileSize ?? resourceValues?.fileSize

        let viewModel = ChatMessageFileCell.ViewModel(
            name: file.name,
            preview: PreviewImageViewModel(url: file.url),
            size: fileSize.map {
                ByteCountFormatter.string(
                    fromByteCount: Int64($0),
                    countStyle: .file
                )
            },
            text: file.text,
            onTap: { actions?.showFile(url) }
        )

        var cellConfig = ChatMessageContainerConfiguration.file(
            viewModel: viewModel,
            layoutType: layoutContext.layoutType
        )

        guard let actions, let text = file.text else {
            return .init(messageId, cellConfig)
        }

        let menuActions: () -> [UIMenuElement] = { [text, actions] in
            let copy = UIAction.chatCopy {
                actions.copy(text)
            }
            return [copy]
        }

        cellConfig.setActions(menuActions)

        return .init(messageId, cellConfig)
    }
}

extension Chat.LocalMessage.Status.OutgoingStatus {
    func configurationStatus() -> ChatMessageStatusViewConfiguration.OutboxStatus {
        switch self {
        case .new: .pending
        case .sent: .sent
        case .delivered: .delivered
        }
    }
}
