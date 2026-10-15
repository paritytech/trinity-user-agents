import ChainRegistry
import Foundation
import Operation_iOS
@testable import PolkadotUI
import Products
import Testing
import UIKit
@testable import polkadot_app

/// The room screen draws the meta of a message (07-LATER §9): a reasoning row instead of the bubble, a branch
/// control and a thread chip under it, the pinned root of a thread, and "Read aloud" in the menu.
@MainActor
struct ChatViewModelFactoryAdditionsTests {
    private typealias Data = ProductRoomTestData

    private let doubles = ChatRoomAdditionsDoubles()

    @Test func aMessageWithoutMetaIsOneBubble() async throws {
        let (factory, _) = try makeFactory()

        let items = try await items(factory, messages: [Data.message("m1")])

        #expect(items.map(\.id) == ["m1"])
        #expect(items.first?.configuration is ChatMessageContainerConfiguration)
    }

    @Test func theBranchControlAndTheThreadChipGoUnderTheBubble() async throws {
        let (factory, presenter) = try makeFactory()
        let meta = Chat.MessageMeta(
            thread: .init(roomId: "thread-lobby-m1", replyCount: 3, lastReplyAt: nil, participantIds: ["p1"]),
            branch: .init(index: 1, count: 2),
            reasoning: nil
        )
        doubles.meta.push(["m1": meta])
        try await waitUntil { presenter.meta(of: "m1") != nil }

        let items = try await items(factory, messages: [Data.message("m1")])

        #expect(items.map(\.id) == ["m1", "m1-branch", "m1-thread"])
        #expect(items.first?.configuration is ChatMessageContainerConfiguration)
        #expect(items.dropFirst().allSatisfy { $0.configuration is SwiftUIContentConfiguration })
    }

    @Test func aReasoningRowReplacesTheBubble() async throws {
        let (factory, presenter) = try makeFactory()
        doubles.meta.push(["m1": Chat.MessageMeta(thread: nil, branch: nil, reasoning: .init(durationMs: 4_000))])
        try await waitUntil { presenter.meta(of: "m1") != nil }

        let items = try await items(factory, messages: [Data.message("m1", content: .text("I think"))])

        #expect(items.map(\.id) == ["m1"])
        #expect(items.first?.configuration is SwiftUIContentConfiguration)
    }

    @Test func aThreadRoomPinsItsRootAboveTheMessages() async throws {
        let (factory, presenter) = try makeFactory(
            chatId: .chatExtension(Data.productId, roomId: "thread-lobby-m1"),
            threadOf: .init(roomId: "lobby", messageId: "m1")
        )
        try await waitUntil { presenter.additions.threadRoot != nil }

        let reply = Chat.LocalMessage.newExtensionMessage(
            Data.productId,
            roomId: "thread-lobby-m1",
            content: .text("Reply 1")
        )
        let viewModel = try await viewModel(factory, messages: [reply])

        #expect(viewModel.sections.flatMap(\.messages).map(\.id).first == "thread-root")
        #expect(viewModel.headerConfiguration.username == "Thread")
        #expect(viewModel.headerConfiguration.additionalInfo == "Lobby")
    }

    @Test func readAloudGoesBeforeDeleteOnlyWhenTheRoomDeclaresIt() throws {
        let (factory, presenter) = try makeFactory()
        let actions: [ChatMessageBubbleAction] = [.copy, .delete]

        #expect(factory.withReadAloud(actions) == actions)

        presenter.update(policy: policy(readAloud: true), messages: [])
        #expect(factory.withReadAloud(actions) == [.copy, .readAloud, .delete])
        #expect(factory.withReadAloud([.copy]) == [.copy, .readAloud])
    }

    @Test func aCustomMenuOfTheProductHasNoReadAloud() {
        let action = Data.action("act-1")

        #expect(ChatMessageBubbleAction.productMenu(hostActions: [.copy, .readAloud], answer: .custom([action]))
            == [.product(action)])
        #expect(ChatMessageBubbleAction.productMenu(
            hostActions: [.copy, .readAloud],
            answer: .extendBuiltIn([action], hidden: [.copy])
        ) == [.readAloud, .product(action)])
    }
}

private extension ChatViewModelFactoryAdditionsTests {
    func makeFactory(
        chatId: Chat.Id = Data.chatId,
        threadOf: Chat.ThreadParent? = nil
    ) throws -> (ChatViewModelFactory, ChatProductRoomPresenter) {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: chatId,
            room: RecordingProductRoom(),
            wireframe: RecordingProductRoomWireframe(),
            resolver: NoImageResolver(),
            activityStore: ChatRoomActivityStore(),
            additions: doubles.dependencies(),
            logger: StubLogger()
        ))
        presenter.update(policy: policy(threadOf: threadOf), messages: [])

        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let dotNs = StubDotNsResolver()
        let factory = try ChatViewModelFactory(
            balanceFactory: TransferAmountViewModelFactory(
                targetAssetInfo: asset.displayInfo,
                formatterFactory: AssetBalanceFormatterFactory()
            ),
            customDecoders: [],
            attachmentViewModelFactory: NoAttachmentItems(),
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
        factory.productRoom = presenter
        return (factory, presenter)
    }

    func policy(threadOf: Chat.ThreadParent? = nil, readAloud: Bool = false) -> ChatRoomPolicy {
        var policy = ChatRoomPolicy(footer: nil, declarations: Data.declarations())
        policy.features = Chat.RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            threadOf: threadOf,
            readAloud: readAloud
        )
        return policy
    }

    func items(
        _ factory: ChatViewModelFactory,
        messages: [Chat.LocalMessage]
    ) async throws -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        try await viewModel(factory, messages: messages).sections.flatMap(\.messages)
    }

    func viewModel(
        _ factory: ChatViewModelFactory,
        messages: [Chat.LocalMessage]
    ) async throws -> ChatViewLayout.ViewModel {
        let listModel = await ChatInteractorState().onMessagesUpdate(messages)
        let chatMetadata = ChatMetadata(
            chatId: factory.productRoom?.chatId ?? Data.chatId,
            peerMetadata: Chat.PeerMetadata(
                name: "Game",
                icon: .bot,
                input: .inputField(.init(canPay: false, canAttachFile: false)),
                moreActions: []
            ),
            state: .created,
            roomPolicy: factory.productRoom?.policy
        )

        return factory.viewModel(
            model: listModel,
            metadata: MessageListMetadata(chatMetadata: chatMetadata, myUsername: "me"),
            actions: .silent,
            footerConfiguration: nil
        )
    }

    func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0 ..< 400 where !condition() {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(condition())
    }
}

private struct NoAttachmentItems: ChatAttachmentViewModelMaking {
    func makeAttachmentItem(
        for _: Chat.LocalMessage.Content.Attachment,
        messageId _: Chat.MessageId,
        onSelection _: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem? {
        nil
    }
}
