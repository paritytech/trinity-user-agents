import ChainRegistry
import Foundation
@testable import PolkadotUI
import Products
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// A product's chat row is drawn from its manifest, the way its name already is.
@Suite("Product bot in the chat list")
struct ProductBotChatListTests {
    @Test
    func aWidgetPreviewsAsTheManifestDescription() {
        let bot = makeBot(description: "Play the weekly game with friends")

        #expect(bot.messageDecoder.previewString(data: Data()) == "Play the weekly game with friends")
    }

    /// A manifest always has a description, but a product with no manifest has none.
    @Test
    func aWidgetOfAProductWithoutADescriptionPreviewsAsAWidget() {
        let bot = makeBot(description: nil)

        #expect(bot.messageDecoder.previewString(data: Data()) == String(localized: .Common.productWidgetMessage))
    }

    /// The list loads the image by domain, so the row shows the manifest icon instead of a letter.
    @Test
    func theRowIconIsTheProductManifestIcon() {
        let bot = makeBot(description: nil)

        #expect(bot.peerMetadata.icon == .product(domain: "dim2.paseo"))
    }

    /// The sender describes each card, so the row says what this one shows rather than what the product is.
    @Test
    func aWidgetWithAnAltPreviewsAsTheAltInTheChatList() throws {
        #expect(try chatListPreview(alt: "Week 12 results") == "Week 12 results")
    }

    @Test
    func aWidgetWithoutAnAltPreviewsAsTheManifestDescriptionInTheChatList() throws {
        #expect(try chatListPreview(alt: nil) == "Play the weekly game with friends")
    }

    @Test
    func aProductPeerLoadsItsIconByDomain() {
        let iconFactory = RecordingProductIconViewModelFactory()

        #expect(Chat.PeerMetadata.Icon.product(domain: "dim2.paseo").imageViewModel(using: iconFactory) != nil)
        #expect(Chat.PeerMetadata.Icon.bot.imageViewModel(using: iconFactory) == nil)
        #expect(iconFactory.requestedDomains == ["dim2.paseo"])
    }
}

private extension ProductBotChatListTests {
    func makeBot(description: String?) -> ProductBot {
        ProductBot(
            product: Product(id: "dim2.paseo", name: "Jollity"),
            description: description,
            runtime: IdleChatRuntime(),
            resolveImage: WidgetImageResolver { _ in nil }
        )
    }

    func chatListPreview(alt: String?) throws -> String? {
        let bot = makeBot(description: "Play the weekly game with friends")
        let chatId = Chat.Id.chatExtension(bot.product.id)
        let message = Chat.LocalMessage(
            messageId: "results",
            chatId: chatId,
            origin: .chatExtension(bot.product.id),
            creationSource: .localDevice,
            status: .incoming(.seen),
            timestamp: 0,
            content: .customRendered(
                Chat.LocalMessage.Content.CustomRenderedData(
                    decoderId: MessageDecoderIdentifier.product.rawValue,
                    data: Data(),
                    identifier: "results",
                    alt: alt
                )
            ),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
        let chat = Chat.LocalModel(
            peer: .chatExtension(bot.product.id),
            message: message,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil
        )
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let factory = ContactsListViewModelFactory(
            chatMessageDecoderFactory: FixedChatMessageDecoderFactory(decoders: [bot.messageDecoder]),
            productIconViewModelFactory: RecordingProductIconViewModelFactory(),
            chain: chain,
            tokenFormatter: { info in
                TransferAmountViewModelFactory(targetAssetInfo: info, formatterFactory: AssetBalanceFormatterFactory())
            }
        )

        let viewModel = factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(
                establishedChats: [ChatWithPeerMetadata(chat: chat, peerMetadata: bot.peerMetadata)],
                pendingIncomingRequestCount: 0,
                newIncomingRequestCount: 0
            )
        )

        return try #require(viewModel.contactsById.first).configuration.message
    }
}

private struct FixedChatMessageDecoderFactory: ChatMessageDecoderMaking {
    let decoders: [ChatMessageCustomDecoding]

    func makeDecoders(for _: ChainModel, chatId _: Chat.Id) -> [ChatMessageCustomDecoding] {
        decoders
    }
}

private final class RecordingProductIconViewModelFactory: ProductIconViewModelMaking {
    private(set) var requestedDomains: [String] = []

    func createViewModel(for domain: String) -> any ImageViewModelProtocol {
        requestedDomains.append(domain)
        return StaticImageViewModel(image: nil)
    }
}

private final class IdleChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {}
    func onUserMessage(text _: String, roomId _: String?) async throws {}

    func renderMessage(
        roomId _: String?,
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(
        roomId _: String?,
        messageId _: String,
        messageType _: String?,
        actionId _: String,
        payload _: String?
    ) async {}

    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
    func dispose() async {}
}
