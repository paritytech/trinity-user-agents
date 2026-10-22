import ChainRegistry
import Foundation
import Operation_iOS
@testable import PolkadotUI
import Products
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// A room of the chat product draws each message with the views and the strings of the native person chat: the
/// product message becomes the content of the native chat, and the native factory draws it (HX2).
@MainActor
struct ContactRoomRenderingTests {
    private typealias Data = ProductRoomTestData
    private let attachments = RecordingAttachmentViewModelFactory()

    @Test func aTextOfTheContactIsATextBubbleOnTheLeft() async throws {
        let bubble = try await bubble(for: message("m1", from: "contact", body: .text(text: "**hi**")))

        #expect(bubble.identifier == ChatMessageTextView.reuseIdentifier)
        #expect(bubble.side == .leading)
        #expect(bubble.statusConfiguration?.image == nil)
    }

    @Test(arguments: [
        (ChatMessageStatus.preparing, ChatMessageStatusViewConfiguration.OutboxStatus.pending),
        (.sent, .sent),
        (.delivered, .delivered),
        (.read, .delivered),
        (.failed, .failed)
    ])
    func eachStatusHasTheNativeMark(status: ChatMessageStatus, mark: ChatMessageStatusViewConfiguration.OutboxStatus) {
        #expect(status.outgoingStatus.configurationStatus() == mark)
    }

    @Test func anOwnTextOffersTheNativeMenu() async throws {
        let bubble = try await bubble(for: message("m1", from: "user", body: .text(text: "mine")))

        #expect(menuTitles(of: bubble) == [
            String(localized: .chatReply),
            String(localized: .chatCopy),
            String(localized: .chatEdit)
        ])
    }

    @Test func anEditedTextOffersItsEditHistory() async throws {
        var edited = message("m1", from: "user", body: .text(text: "mine"))
        edited.editedAt = 2_000

        let bubble = try await bubble(for: edited, editedAt: ["m1": 2_000])

        // The menu item of the app, not the title of the edit history screen of PolkadotUI.
        #expect(menuTitles(of: bubble).last == UIAction.chatEditHistory {}.title)
    }

    @Test func aTextOfTheContactOffersNoEdit() async throws {
        let bubble = try await bubble(for: message("m1", from: "contact", body: .text(text: "hers")))

        #expect(menuTitles(of: bubble) == [String(localized: .chatReply), String(localized: .chatCopy)])
    }

    @Test func theWelcomeMessageOfARequestHasTheRequestRow() async throws {
        var request = message("m1", from: "user", body: .text(text: "Hello there"))
        request.request = true

        let items = try await items(for: [request], state: .outgoing)

        #expect(items.map(\.id) == ["m1-header", "m1"])
        let row = try #require(items[0].configuration as? ChatInfoMessageConfiguration)
        #expect(row.attributedText.string == ChatInfoMessageConfiguration.chatRequested().attributedText.string)
    }

    @Test func theEventsHaveTheNativeRows() async throws {
        let accepted = message("m1", from: "contact", body: .event(.requestAccepted))
        let added = message("m2", from: "user", body: .event(.contactAdded))
        let left = message("m3", from: "contact", body: .event(.leftChat))

        let items = try await items(for: [accepted, added, left])

        let rows: [ChatInfoMessageConfiguration] = [
            .chatRequestAccepted(by: "alice"),
            .youAdded(username: "alice"),
            .leftChat(username: "alice")
        ]
        let drawn = items.compactMap { $0.configuration as? ChatInfoMessageConfiguration }
        #expect(drawn.map(\.attributedText.string) == rows.map(\.attributedText.string))
    }

    @Test func messagesThatCouldNotBeRetrievedShowNothing() async throws {
        let items = try await items(for: [message("m1", from: "contact", body: .event(.unavailable))])

        #expect(items.isEmpty)
    }

    @Test func anUnsupportedMessageIsTheNativeBubble() async throws {
        let bubble = try await bubble(for: message("m1", from: "contact", body: .event(.unsupported)))

        #expect(bubble.identifier == ChatMessageTextView.reuseIdentifier)
        #expect(menuTitles(of: bubble).isEmpty)
    }

    @Test func aPaymentIsTheNativeTransferBubble() async throws {
        let payment = message(
            "m1",
            from: "contact",
            body: .payment(ChatPaymentRecord(amount: "1000000", legacy: false))
        )

        let items = try await items(for: [payment])
        let bubble = try #require(items.first?.configuration as? ChatMessageContainerConfiguration)

        #expect(bubble.innerContentConfiguration is ChatTransferMessageConfiguration)
    }

    @Test func aCallIsTheNativeCallBubble() async throws {
        let call = message("m1", from: "contact", body: .call(ChatCallRecord(
            video: true,
            answeredAt: nil,
            closed: ChatCallClose(at: 5_000, byUser: false)
        )))

        let items = try await items(for: [call])
        let bubble = try #require(items.first?.configuration as? ChatMessageContainerConfiguration)
        let content = try #require(bubble.innerContentConfiguration as? ChatCallMessageConfiguration)

        #expect(content.callType == .video)
        #expect(content.direction == .incoming)
        #expect(content.state == .missed)
        #expect(content.onTap != nil)
    }

    @Test func aMediaMessageIsTheNativeMediaBubble() async throws {
        let media = message("m1", from: "contact", body: .richText(text: "Look", media: [
            .image(
                resource: .hop(
                    identifier: Foundation.Data([1]),
                    ticket: Foundation.Data([2]),
                    node: "wss://hop",
                    size: 3
                ),
                meta: ChatMediaMeta(
                    mimeType: "image/png",
                    sizeBytes: 3,
                    width: 4,
                    height: 5,
                    placeholder: "L",
                    alt: nil,
                    durationMs: nil
                )
            )
        ]))

        _ = try await bubble(for: media)

        // The native media grid downloads the photo from its HOP node.
        let attachment = try #require(attachments.requested.first)
        guard case .remoteDownloadable(.p2pMixnet) = attachment else {
            Issue.record("the photo is not a HOP file of the native chat")
            return
        }
        #expect(attachments.requested.count == 1)
    }
}

private extension ContactRoomRenderingTests {
    func message(_ id: String, from participantId: String, body: ChatMessageBody) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: participantId),
            timestamp: 1_000,
            body: body,
            status: participantId == "user" ? .sent : .read,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
    }

    func bubble(
        for message: ChatMessage,
        editedAt: [Chat.MessageId: Chat.Timestamp] = [:]
    ) async throws -> ChatMessageContainerConfiguration {
        let items = try await items(for: [message], editedAt: editedAt)
        let item = try #require(items.first { $0.id == message.messageId })
        return try #require(item.configuration as? ChatMessageContainerConfiguration)
    }

    func items(
        for messages: [ChatMessage],
        state: Chat.RoomContact.State = .established,
        editedAt: [Chat.MessageId: Chat.Timestamp] = [:]
    ) async throws -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        let localMessages = messages.map { $0.localMessage(
            chatId: Data.chatId,
            extensionId: Data.productId,
            isSeen: false
        ) }
        let listModel = await ChatInteractorState().onSnapshot(MessageListSnapshot(
            messages: localMessages,
            editedAt: editedAt
        ))
        let declarations = Data.declarations(
            config: Data.config(productSource: true),
            contact: Data.contact(state)
        )
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations)
        let contact = Data.contact(state)
        let chatMetadata = ChatMetadata(
            chatId: Data.chatId,
            peerMetadata: contact.peerMetadata(name: "alice", policy: policy),
            state: .created,
            roomPolicy: policy
        )

        return try makeFactory().viewModel(
            model: listModel,
            metadata: MessageListMetadata(chatMetadata: chatMetadata, myUsername: "me"),
            actions: .silent,
            footerConfiguration: nil
        )
        .sections
        .flatMap(\.messages)
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
            attachmentViewModelFactory: attachments,
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

/// Records the attachments that the bubbles draw, and draws none, so a media bubble shows without the stores of the
/// app.
private final class RecordingAttachmentViewModelFactory: ChatAttachmentViewModelMaking, @unchecked Sendable {
    private(set) var requested: [Chat.LocalMessage.Content.Attachment] = []

    func makeAttachmentItem(
        for attachment: Chat.LocalMessage.Content.Attachment,
        messageId _: Chat.MessageId,
        onSelection _: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem? {
        requested.append(attachment)
        return nil
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
            processAction: { _ in },
            selectAttachment: { _ in },
            unblockUser: {},
            startCall: { _ in },
            openProduct: { _ in }
        )
    }
}
