import ExternalAccessibility
import Foundation
import PolkadotUI
import Foundation_iOS
import SubstrateSdk
import ChainRegistry

protocol ContactsListViewModelMaking {
    func createViewModel(
        assetDisplayInfo: AssetBalanceDisplayInfo,
        model: ChatListModel
    ) -> ContactsListViewLayout.ViewModel

    /// The rooms and the messages that the search of the list found (P10.5).
    func createSearchViewModel(
        assetDisplayInfo: AssetBalanceDisplayInfo,
        results: ChatListSearchResults
    ) -> ContactsListViewLayout.ViewModel
}

final class ContactsListViewModelFactory {
    let messageTimestampFormatter: TimestampFormatting
    let chatMessageDecoderFactory: ChatMessageDecoderMaking
    let productIconViewModelFactory: ProductIconViewModelMaking
    let resources: ChatResourceResolving
    let calls: ChatRoomCallStoring
    let chain: ChainModel
    let tokenFormatter: (AssetBalanceDisplayInfo) -> TransferAmountViewModelFactoryProtocol
    private var _tokenFormatter: TransferAmountViewModelFactoryProtocol?

    init(
        messageTimestampFormatter: TimestampFormatting = ContactTimestampFormatter(),
        chatMessageDecoderFactory: ChatMessageDecoderMaking,
        productIconViewModelFactory: ProductIconViewModelMaking,
        resources: ChatResourceResolving = ChatResourceResolver.shared,
        calls: ChatRoomCallStoring = ChatRoomCallStore.shared,
        chain: ChainModel,
        tokenFormatter: @escaping (AssetBalanceDisplayInfo) -> TransferAmountViewModelFactoryProtocol
    ) {
        self.messageTimestampFormatter = messageTimestampFormatter
        self.chatMessageDecoderFactory = chatMessageDecoderFactory
        self.productIconViewModelFactory = productIconViewModelFactory
        self.resources = resources
        self.calls = calls
        self.chain = chain
        self.tokenFormatter = tokenFormatter
    }
}

extension ContactsListViewModelFactory: ContactsListViewModelMaking {
    func createViewModel(
        assetDisplayInfo: AssetBalanceDisplayInfo,
        model: ChatListModel
    ) -> ContactsListViewLayout.ViewModel {
        // A thread room opens from the message that it answers, not from the list (07-LATER §9.2).
        let chats = model.establishedChats.filter { $0.chat.roomDeclarations?.features.threadOf == nil }

        return ContactsListViewLayout.ViewModel(
            contactsById: chats.map { row(for: $0, assetDisplayInfo: assetDisplayInfo) },
            pendingIncomingRequestCount: model.pendingIncomingRequestCount,
            newIncomingRequestCount: model.newIncomingRequestCount,
            archivedRow: model.archivedChatCount > 0
                ? ChatListArchivedRowConfiguration(
                    title: String(localized: .ChatExtension.listArchived),
                    count: model.archivedChatCount
                )
                : nil
        )
    }

    func createSearchViewModel(
        assetDisplayInfo: AssetBalanceDisplayInfo,
        results: ChatListSearchResults
    ) -> ContactsListViewLayout.ViewModel {
        let hits = results.hits.map { hit in
            let excerpt = ChatSearchExcerpt(text: hit.hit.text, query: results.query)
            let configuration = DSChatListItemConfiguration(
                dateFormatter: messageTimestampFormatter,
                avatarViewModel: avatarViewModel(for: hit.chat),
                icon: hit.chat.peerMetadata.icon.imageViewModel(using: productIconViewModelFactory),
                sender: hit.chat.peerMetadata.name,
                message: excerpt.line,
                date: Date.fromChatTimestamp(hit.hit.timestamp)
            )
            return IdentifiableContentConfiguration(id: hit.identifier, configuration: configuration)
        }

        return ContactsListViewLayout.ViewModel(
            contactsById: results.chats.map { row(for: $0, assetDisplayInfo: assetDisplayInfo) },
            pendingIncomingRequestCount: 0,
            newIncomingRequestCount: 0,
            messageHits: ContactsListViewLayout.MessageHits(
                title: String(localized: .ChatExtension.listSearchMessages),
                hits: hits
            )
        )
    }
}

private extension ContactsListViewModelFactory {
    func row(
        for chatWithPeerMetadata: ChatWithPeerMetadata,
        assetDisplayInfo: AssetBalanceDisplayInfo
    ) -> IdentifiableContentConfiguration<String, DSChatListItemConfiguration> {
        let chat = chatWithPeerMetadata.chat
        let peerMetadata = chatWithPeerMetadata.peerMetadata
        let preview = rowPreview(for: chatWithPeerMetadata, assetDisplayInfo: assetDisplayInfo)
        let badge = rowBadge(for: chat)

        let configuration = DSChatListItemConfiguration(
            dateFormatter: messageTimestampFormatter,
            avatarViewModel: avatarViewModel(for: chatWithPeerMetadata),
            icon: peerMetadata.icon.imageViewModel(using: productIconViewModelFactory),
            sender: peerMetadata.name,
            message: preview.text,
            messageKind: preview.kind,
            date: rowDate(for: chat),
            isMuted: chat.roomPolicy?.userState.isMuted(at: Date()) ?? false,
            hasReaction: badge.hasReaction,
            unreadCount: badge.unreadCount,
            badge: badge.productBadge,
            isOngoing: preview.isOngoing,
            accessibilityId: nil
        )
        return IdentifiableContentConfiguration(id: chat.identifier, configuration: configuration)
    }

    func avatarViewModel(for chatWithPeerMetadata: ChatWithPeerMetadata) -> AvatarViewModel {
        let peerMetadata = chatWithPeerMetadata.peerMetadata
        if let image = peerMetadata.icon.image {
            return .image(image)
        }

        let prefix = String(peerMetadata.name.prefix(1))
        return .colored(text: prefix, colorSeed: chatWithPeerMetadata.chat.chatId.colorSeed)
    }
}

/// The text of a row and how the row draws it.
private struct ChatListRowPreview {
    let text: String?
    let kind: DSChatMessage.Kind
    let isOngoing: Bool
}

/// The badges of a row. A product badge replaces the reaction and the unread badges of the host.
private struct ChatListRowBadge {
    let hasReaction: Bool
    let unreadCount: Int
    let productBadge: DSChatListBadge.Kind?
}

private extension ContactsListViewModelFactory {
    func rowPreview(
        for chatWithPeerMetadata: ChatWithPeerMetadata,
        assetDisplayInfo: AssetBalanceDisplayInfo
    ) -> ChatListRowPreview {
        let chat = chatWithPeerMetadata.chat

        if let ringing = ChatRoomCallViewModel.listPreview(of: calls.call(for: chat.chatId)) {
            return ChatListRowPreview(text: ringing, kind: .default, isOngoing: true)
        }

        switch chat.roomPolicy?.listPreview ?? .lastMessage {
        case .lastMessage:
            return ChatListRowPreview(
                text: lastMessageText(for: chatWithPeerMetadata, assetDisplayInfo: assetDisplayInfo),
                kind: messageKind(for: chat.message),
                isOngoing: false
            )
        case let .text(text, ongoing):
            return ChatListRowPreview(text: text, kind: .default, isOngoing: ongoing)
        }
    }

    /// The timestamp of the product wins over the time of the last message.
    func rowDate(for chat: Chat.LocalModel) -> Date? {
        let timestamp = chat.productTimestamp ?? chat.message?.timestamp
        return timestamp.map(Date.fromChatTimestamp)
    }

    func rowBadge(for chat: Chat.LocalModel) -> ChatListRowBadge {
        switch chat.roomPolicy?.listBadge ?? .hostUnreadCount {
        case .hostUnreadCount:
            ChatListRowBadge(
                hasReaction: chat.hasIncomingReaction,
                unreadCount: chat.unreadDisplayMessageCount,
                productBadge: nil
            )
        case .hidden:
            ChatListRowBadge(hasReaction: false, unreadCount: 0, productBadge: .hidden)
        case let .count(count):
            ChatListRowBadge(hasReaction: false, unreadCount: 0, productBadge: .counter(count))
        case let .image(image):
            ChatListRowBadge(
                hasReaction: false,
                unreadCount: 0,
                productBadge: .image(DSChatListBadge.ImageSource(
                    id: image.identifier,
                    viewModel: ChatResourceImageViewModel(resource: image.resource, resolver: resources)
                ))
            )
        }
    }

    func messageKind(for message: Chat.LocalMessage?) -> DSChatMessage.Kind {
        guard let message else { return .default }

        switch message.content {
        case .richText,
             .staticTextImageContent,
             .file:
            return .media
        case .productBody(.file):
            return .media
        case let .productBody(.richText(richText)) where !richText.media.isEmpty:
            return .media
        case .call:
            return callMessageKind(for: message)
        default:
            return .default
        }
    }

    func callMessageKind(for message: Chat.LocalMessage) -> DSChatMessage.Kind {
        guard
            let offerMessage = message.callOffer,
            case let .call(.offer(offer)) = offerMessage.content
        else {
            return .default
        }

        switch offer.purpose {
        case .audio: return .voiceCall
        case .video: return .videoCall
        }
    }

    func reactedText(
        for message: Chat.LocalMessage,
        peerMetadata: Chat.PeerMetadata,
        content: Chat.RemoteMessageContentV1.MessageContent.ReactionContent
    ) -> String {
        switch message.status {
        case .incoming:
            String(
                localized: .chatLastMessageReactedIncoming(
                    username: peerMetadata.name,
                    emoji: content.emoji
                )
            )
        case .outgoing:
            String(localized: .chatLastMessageReactedOutgoing(emoji: content.emoji))
        }
    }

    func reactionRemoved(for message: Chat.LocalMessage, peerMetadata: Chat.PeerMetadata) -> String {
        switch message.status {
        case .incoming:
            String(localized: .chatLastMessageReactionRemovedIncoming(username: peerMetadata.name))
        case .outgoing:
            String(localized: .chatLastMessageReactionRemovedOutgoing)
        }
    }

    func latestEditText(for message: Chat.LocalMessage) -> String? {
        message.relatedMessages
            .compactMap { related -> (timestamp: Chat.Timestamp, text: String?)? in
                guard case let .edited(content) = related.content else { return nil }
                return (related.timestamp, content.newContent.text)
            }
            .max(by: { $0.timestamp < $1.timestamp })?
            .text
    }

    func displayRichText(for richText: Chat.LocalMessage.Content.RichText) -> String {
        let displayText = richText.text ?? ""

        guard displayText.isEmpty, richText.attachments?.first != nil else {
            return displayText
        }

        return String(localized: .chatLastMessageMediaFile)
    }

    func productBodyPreview(_ body: Chat.LocalMessage.Content.ProductBody) -> String {
        if let text = body.text, !text.isEmpty {
            return text
        }

        switch body {
        case .richText: return String(localized: .chatLastMessageMediaFile)
        case let .file(file): return file.fileName
        case .actions: return String(localized: .ChatExtension.messagePreviewActions)
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    func lastMessageText(
        for chatWithPeerMetadata: ChatWithPeerMetadata,
        assetDisplayInfo: AssetBalanceDisplayInfo
    ) -> String? {
        let chat = chatWithPeerMetadata.chat
        let peerMetadata = chatWithPeerMetadata.peerMetadata

        guard let message = chat.message else {
            return nil
        }

        if let editedText = latestEditText(for: message) {
            return String(localized: .chatLastMessageEdited(text: editedText))
        }

        switch message.content {
        case let .text(text):
            return text

        case let .richText(richText):
            return displayRichText(for: richText)

        case let .staticTextImageContent(content):
            return content.text ?? String(localized: .ChatExtension.messagePreviewImage)

        case .token:
            return nil

        case let .send(content) where _tokenFormatter == nil:
            let formatter = tokenFormatter(assetDisplayInfo)
            _tokenFormatter = formatter
            fallthrough

        case let .send(content):
            return makeSendMessage(
                for: content.amount,
                tokenFormatter: _tokenFormatter!,
                messageStatus: message.status,
                peerName: peerMetadata.name
            )

        case let .coinageSend(content) where _tokenFormatter == nil:
            let formatter = tokenFormatter(assetDisplayInfo)
            _tokenFormatter = formatter
            fallthrough

        case let .coinageSend(content):
            return makeSendMessage(
                for: content.totalValue,
                tokenFormatter: _tokenFormatter!,
                messageStatus: message.status,
                peerName: peerMetadata.name
            )

        case .contactAdded:
            switch message.status {
            case .incoming:
                return String(localized: .chatContactAddedYou(username: peerMetadata.name))
            case .outgoing:
                return String(localized: .chatYouAddedContact(username: peerMetadata.name))
            }

        case .leftChat:
            switch message.status {
            case .incoming:
                return String(localized: .chatPeerLeft(username: peerMetadata.name))
            case .outgoing:
                return String(localized: .chatYouLeft)
            }

        case let .reply(replyContent):
            return replyContent.ownContent.text ?? ""

        case .reacted,
             .reactionRemoved,
             .compactedMessages,
             .deleted:
            return nil

        case let .edited(editedContent):
            return String(localized: .chatLastMessageEdited(text: editedContent.newContent.text ?? ""))

        case .chatAccepted,
             .multiChatAccepted:
            if message.status.isIncoming {
                return String(localized: .chatListRequestAcceptedByPeer)
            } else {
                return String(localized: .chatListRequestAcceptedByYou)
            }

        case let .chatRequest(content):
            return content.welcomeMessage?.text ?? String(localized: .chatRequestSent)

        case let .versionedChatRequest(content):
            return content.ensureV1().welcomeMessage?.text ?? String(localized: .chatRequestSent)

        case let .extensionActionResponse(content, _):
            return content

        case let .customRendered(content):
            if let alt = content.alt {
                return alt
            }

            let decoders = chatMessageDecoderFactory.makeDecoders(for: chain, chatId: chat.chatId)
            let preview = decoders
                .first(where: { $0.identifier.rawValue == content.decoderId })?
                .previewString(data: content.data)
            return preview ?? ""

        case let .file(file):
            return file.name

        case let .productBody(body):
            return productBodyPreview(body)

        case .unsupported:
            return String(localized: .chatUnsupportedContent)

        // TODO: Design a user-facing representation for device change events
        case .deviceAdded,
             .deviceRemoved:
            return nil

        case .call:
            return callPreview(for: message)
        }
    }

    func callPreview(for message: Chat.LocalMessage) -> String? {
        guard
            let offerMessage = message.callOffer,
            case let .call(.offer(offer)) = offerMessage.content,
            let callState = message.resolveCallState()
        else {
            return nil
        }

        let callType: ChatCallMessageConfiguration.CallType =
            switch offer.purpose {
            case .audio: .audio
            case .video: .video
            }

        let direction: ChatCallMessageConfiguration.Direction =
            switch offerMessage.status {
            case .incoming: .incoming
            case .outgoing: .outgoing
            }

        let uiState: ChatCallMessageConfiguration.State =
            switch callState {
            case .calling:
                .calling
            case .active:
                .active
            case let .finished(duration):
                .finished(duration: formatCallDuration(duration))
            case let .cancelled(duration):
                .cancelled(ringDuration: formatCallDuration(duration))
            case .missed:
                .missed
            }

        return ChatCallMessageConfiguration.title(
            direction: direction,
            callType: callType,
            state: uiState
        )
    }

    func formatCallDuration(_ duration: UInt64) -> String {
        let seconds = TimeInterval(duration) / 1_000
        return DateComponentsFormatter.secondsMinutesAbbreviated.string(from: seconds) ?? ""
    }

    func makeSendMessage(
        for amount: Balance,
        tokenFormatter: TransferAmountViewModelFactoryProtocol,
        messageStatus: Chat.LocalMessage.Status,
        peerName: String
    ) -> String {
        let formattedAmount = tokenFormatter.amount(from: amount)
        let message =
            switch messageStatus {
            case .incoming:
                String(localized: .chatTransferReceived(contact: peerName, amount: formattedAmount))
            case .outgoing:
                String(localized: .chatTransferSent(amount: formattedAmount))
            }

        return message
    }
}
