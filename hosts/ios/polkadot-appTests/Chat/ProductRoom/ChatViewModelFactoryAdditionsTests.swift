import ChainRegistry
import Foundation
import Operation_iOS
@testable import PolkadotUI
import Products
import Testing
import UIKit
@testable import polkadot_app

/// The room screen draws each message as one bubble, and offers "Read aloud" in the menu (07-LATER §9).
@MainActor
struct ChatViewModelFactoryAdditionsTests {
    private typealias Data = ProductRoomTestData

    private let doubles = ChatRoomAdditionsDoubles()

    @Test func aMessageIsOneBubble() async throws {
        let (factory, _) = try makeFactory()

        let items = try await items(factory, messages: [Data.message("m1")])

        #expect(items.map(\.id) == ["m1"])
        #expect(items.first?.configuration is ChatMessageContainerConfiguration)
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
    func makeFactory() throws -> (ChatViewModelFactory, ChatProductRoomPresenter) {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: RecordingProductRoom(),
            wireframe: RecordingProductRoomWireframe(),
            resolver: NoImageResolver(),
            additions: doubles.dependencies(),
            logger: StubLogger()
        ))
        presenter.update(policy: policy(), messages: [])

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

    func policy(readAloud: Bool = false) -> ChatRoomPolicy {
        var policy = ChatRoomPolicy(footer: nil, declarations: Data.declarations())
        policy.features = Chat.RoomFeatures(preimageAttachments: false, search: false, calls: nil, readAloud: readAloud)
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
