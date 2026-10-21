import ChainRegistry
import Foundation
import Operation_iOS
@testable import PolkadotUI
import Products
import Testing
import UIKit
@testable import polkadot_app

/// The room screen draws each product body and each status (02-DESIGN §6, P4.2).
@MainActor
struct ChatViewModelFactoryTests {
    private let roomId = Chat.Id.chatExtension("game.dot", roomId: "lobby")

    @Test func aRichTextWithoutMediaIsATextBubble() async throws {
        let bubble = try await bubble(for: product(.richText(.init(text: "**hi**", media: []))))

        #expect(bubble.identifier == ChatMessageTextView.reuseIdentifier)
        #expect(bubble.side == .leading)
    }

    @Test func aRichTextWithMediaIsAMediaBubble() async throws {
        let body = Chat.LocalMessage.Content.ProductBody.richText(
            .init(text: "Look", media: ["https://example.com/a.png"])
        )

        let bubble = try await bubble(for: product(body))

        #expect(bubble.identifier == ChatMediaStackView.reuseIdentifier)
        #expect(bubble.side == .leading)
    }

    @Test func actionsAreAnActionsBubble() async throws {
        let body = Chat.LocalMessage.Content.ProductBody.actions(.init(
            text: "Pick one",
            buttons: [.init(actionId: "a", title: "A")],
            layout: .column
        ))

        let bubble = try await bubble(for: product(body))

        #expect(bubble.identifier == ChatActionsMessageView.reuseIdentifier)
    }

    @Test func aFileIsAFileBubble() async throws {
        let body = Chat.LocalMessage.Content.ProductBody.file(.init(
            url: "https://example.com/r.pdf",
            fileName: "r.pdf",
            mimeType: "application/pdf",
            sizeBytes: 4_096,
            text: nil
        ))

        let bubble = try await bubble(for: product(body))

        #expect(bubble.identifier == ChatMessageFileCell.reuseIdentifier)
    }

    @Test func aTombstoneDrawsNothing() async throws {
        let message = product(nil)
        let viewModel = try await viewModel(for: [message])

        #expect(viewModel.sections.flatMap(\.messages).isEmpty)
        #expect(try !makeFactory().isMessageVisible(message))
    }

    @Test(arguments: [
        (Chat.LocalMessage.Status.OutgoingStatus.new, ChatMessageStatusViewConfiguration.OutboxStatus.pending),
        (.sent, .sent),
        (.delivered, .delivered),
        (.read, .read),
        (.failed, .failed)
    ])
    func eachOutgoingStatusHasItsMark(
        status: Chat.LocalMessage.Status.OutgoingStatus,
        mark: ChatMessageStatusViewConfiguration.OutboxStatus
    ) async throws {
        #expect(status.configurationStatus() == mark)

        let bubble = try await bubble(for: own(status: status))
        #expect(bubble.side == .trailing)
        #expect(bubble.statusConfiguration?.image != nil)
    }

    @Test func anIncomingMessageHasNoMark() async throws {
        let bubble = try await bubble(for: product(.richText(.init(text: "hi", media: []))))

        #expect(bubble.statusConfiguration?.image == nil)
    }

    @Test func aFailedMessageOffersRetryInARoomWhoseProductKeepsTheMessages() async throws {
        let bubble = try await bubble(for: own(status: .failed), policy: policy(store: .product))

        #expect(menuTitles(of: bubble).contains(String(localized: .Common.retry)))
    }

    @Test func aFailedMessageOffersNoRetryInARoomThatTheHostStores() async throws {
        let bubble = try await bubble(for: own(status: .failed), policy: policy(store: .host))

        #expect(!menuTitles(of: bubble).contains(String(localized: .Common.retry)))
    }

    @Test func anOwnMessageOfAProductRoomEndsItsMenuWithDelete() async throws {
        let bubble = try await bubble(for: own(status: .delivered))

        #expect(menuTitles(of: bubble).last == String(localized: .ChatExtension.messageDelete))
    }

    @Test func aProductMessageOffersDeleteToo() async throws {
        let body = Chat.LocalMessage.Content.ProductBody.actions(.init(
            text: "Pick one",
            buttons: [.init(actionId: "a", title: "A")],
            layout: .grid
        ))

        let bubble = try await bubble(for: product(body))

        #expect(menuTitles(of: bubble) == [
            String(localized: .chatReply),
            String(localized: .chatCopy),
            String(localized: .ChatExtension.messageDelete)
        ])
    }

    @Test func aPersonChatOffersNoDelete() async throws {
        let bubble = try await bubble(for: own(status: .delivered), policy: nil)

        #expect(!menuTitles(of: bubble).contains(String(localized: .ChatExtension.messageDelete)))
    }

    @Test(arguments: ChatViewModelFactoryTests.productBodies)
    func aProductBubbleOffersReplyAndReactions(body: Chat.LocalMessage.Content.ProductBody) async throws {
        let bubble = try await bubble(for: product(body))

        #expect(bubble.canReply)
        #expect(bubble.addReaction?.quickEmojis == Chat.quickReactions)
        #expect(menuTitles(of: bubble).first == String(localized: .chatReply))
    }

    @Test func theReactionsOnAProductBubbleShow() async throws {
        let message = product(Self.productBodies[1])
        let reacted = Chat.LocalMessage(
            messageId: message.messageId,
            chatId: message.chatId,
            origin: message.origin,
            creationSource: message.creationSource,
            status: message.status,
            timestamp: message.timestamp,
            content: message.content,
            reactions: [.init(messageId: message.messageId, emoji: "🎉", origin: .user, chatId: roomId, timestamp: 1)],
            compactionId: nil,
            relatedMessages: []
        )

        let bubble = try await bubble(for: reacted)

        #expect(bubble.messageReaction?.reactions.map(\.emoji) == ["🎉"])
    }

    @Test func thePickerOffersOnlyTheEmojiThatTheProductAllows() async throws {
        let allowed = ["👍", "🎉"]

        let own = try await bubble(
            for: own(status: .delivered),
            policy: policy(store: .host, allowedReactions: allowed)
        )
        let product = try await bubble(
            for: product(Self.productBodies[0]),
            policy: policy(store: .host, allowedReactions: allowed)
        )

        for bubble in [own, product] {
            #expect(bubble.addReaction?.quickEmojis == allowed)
            #expect(bubble.addReaction?.allSections.flatMap(\.emojis) == allowed)
        }
    }

    @Test func aRoomThatAllowsNoEmojiShowsNoPicker() async throws {
        let bubble = try await bubble(for: own(status: .delivered), policy: policy(store: .host, allowedReactions: []))

        #expect(bubble.addReaction == nil)
    }

    @Test func aReplyQuotesTheProductBody() async throws {
        let target = product(Self.productBodies[1])
        let reply = Chat.LocalMessage.newMessage(to: roomId, content: .reply(.init(
            messageId: target.messageId,
            ownContent: ChatRemoteMessageContent.RichText(text: "ok", attachments: nil)
        )))
        .replacingStatus(.outgoing(.delivered))

        let viewModel = try await viewModel(for: [target, reply])
        let item = try #require(viewModel.sections.flatMap(\.messages).first { $0.id == reply.messageId })
        let bubble = try #require(item.configuration as? ChatMessageContainerConfiguration)

        #expect(bubble.replyInfo?.preview == "Pick one")
    }

    @Test func anEmptyProductRoomShowsTheHostText() async throws {
        let viewModel = try await viewModel(for: [], policy: .defaults)

        #expect(viewModel.placeholderText == String(localized: .ChatExtension.roomPlaceholder))
    }

    @Test func anEmptyProductRoomShowsTheTextOfItsProduct() async throws {
        let viewModel = try await viewModel(for: [], policy: policy(store: .host, placeholder: "Say hi"))

        #expect(viewModel.placeholderText == "Say hi")
    }

    @Test func aPersonChatShowsNoPlaceholder() async throws {
        let viewModel = try await viewModel(for: [], policy: nil)

        #expect(viewModel.placeholderText == nil)
    }
}

extension ChatViewModelFactoryTests {
    static let productBodies: [Chat.LocalMessage.Content.ProductBody] = [
        .richText(.init(text: "Look", media: ["https://example.com/a.png"])),
        .actions(.init(text: "Pick one", buttons: [.init(actionId: "a", title: "A")], layout: .column)),
        .file(.init(
            url: "https://example.com/r.pdf",
            fileName: "r.pdf",
            mimeType: "application/pdf",
            sizeBytes: 4_096,
            text: nil
        ))
    ]
}

private extension ChatViewModelFactoryTests {
    /// A product message with the body, or a tombstone for nil.
    func product(_ body: Chat.LocalMessage.Content.ProductBody?) -> Chat.LocalMessage {
        Chat.LocalMessage.newExtensionMessage(
            "game.dot",
            roomId: "lobby",
            content: body.map { .productBody($0) } ?? .deleted
        )
    }

    func own(status: Chat.LocalMessage.Status.OutgoingStatus) -> Chat.LocalMessage {
        Chat.LocalMessage.newMessage(to: roomId, content: .text("mine")).replacingStatus(.outgoing(status))
    }

    func policy(
        store: ChatRoomPolicy.MessageStore,
        placeholder: String? = nil,
        allowedReactions: [String]? = nil
    ) -> ChatRoomPolicy {
        let defaults = ChatRoomPolicy.defaults
        return ChatRoomPolicy(
            messageStore: store,
            listPreview: defaults.listPreview,
            listBadge: defaults.listBadge,
            listOrder: defaults.listOrder,
            reportsSeen: defaults.reportsSeen,
            messageMenu: .init(builtIns: defaults.messageMenu.builtIns, allowedReactions: allowedReactions),
            participants: defaults.participants,
            offersReactionDetails: defaults.offersReactionDetails,
            toolbar: defaults.toolbar,
            footer: defaults.footer,
            placeholder: placeholder,
            hostAppliesUserActions: defaults.hostAppliesUserActions
        )
    }

    func bubble(
        for message: Chat.LocalMessage,
        policy: ChatRoomPolicy? = .defaults
    ) async throws -> ChatMessageContainerConfiguration {
        let viewModel = try await viewModel(for: [message], policy: policy)
        let item = try #require(viewModel.sections.flatMap(\.messages).first { $0.id == message.messageId })
        return try #require(item.configuration as? ChatMessageContainerConfiguration)
    }

    func viewModel(
        for messages: [Chat.LocalMessage],
        policy: ChatRoomPolicy? = .defaults
    ) async throws -> ChatViewLayout.ViewModel {
        let listModel = await ChatInteractorState().onMessagesUpdate(messages)
        let chatMetadata = ChatMetadata(
            chatId: roomId,
            peerMetadata: Chat.PeerMetadata(
                name: "Game",
                icon: .bot,
                input: .inputField(.init(canPay: false, canAttachFile: false)),
                moreActions: []
            ),
            state: .created,
            roomPolicy: policy
        )

        return try makeFactory().viewModel(
            model: listModel,
            metadata: MessageListMetadata(chatMetadata: chatMetadata, myUsername: "me"),
            actions: .silent,
            footerConfiguration: nil
        )
    }

    func makeFactory() throws -> ChatViewModelFactory {
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let dotNs = StubDotNsResolver()

        return try ChatViewModelFactory(
            balanceFactory: TransferAmountViewModelFactory(
                targetAssetInfo: asset.displayInfo,
                formatterFactory: AssetBalanceFormatterFactory()
            ),
            customDecoders: [],
            attachmentViewModelFactory: NoAttachmentViewModelFactory(),
            productRepository: AnyDataProviderRepository(InMemoryDataProviderRepository<Product>()),
            productNameCache: ProductNameCache(),
            flowState: SPAFlowState(
                dotNsResolver: dotNs,
                productImages: ProductImageSources(
                    dotNsResolver: dotNs,
                    ipfsGatewayBaseUrl: #require(URL(string: "https://ipfs.example.com"))
                ),
                hostProvider: StubProductHostProvider(syncHostResult: nil, asyncHostResult: .success(nil)),
                productResolver: StubProductResolver(),
                iconViewModelFactory: RecordingProductIconViewModelFactory()
            )
        )
    }

    func menuTitles(of bubble: ChatMessageContainerConfiguration) -> [String] {
        (bubble.menuProvider?() ?? []).map(\.title)
    }
}

private struct NoAttachmentViewModelFactory: ChatAttachmentViewModelMaking {
    func makeAttachmentItem(
        for _: Chat.LocalMessage.Content.Attachment,
        messageId _: Chat.MessageId,
        onSelection _: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem? {
        nil
    }
}

extension ChatViewModelActions {
    /// Actions that do nothing, for view models that a test only reads.
    static var silent: ChatViewModelActions {
        ChatViewModelActions(
            reply: { _ in },
            copy: { _ in },
            edit: { _ in },
            toggleReaction: { _, _ in },
            showReactionDetails: { _ in },
            acceptChatRequest: {},
            declineChatRequest: {},
            showEditHistory: { _ in },
            showFile: { _ in },
            openLink: { _ in },
            processAction: { _ in },
            selectAttachment: { _ in },
            unblockUser: {},
            startCall: { _ in },
            openProduct: { _ in },
            retry: { _ in },
            delete: { _ in }
        )
    }
}
