import ChainRegistry
import Foundation
@testable import PolkadotUI
import Products
import Testing
import TrUAPIHost
import UIKit
import UIKitExt
@testable import polkadot_app

private let productId = try! ProductId("dim2.paseo")

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

        #expect(bot.peerMetadata.icon == .product(domain: productId))
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
        let publicProfileFactory = UnusedPublicProfileAvatarFactory()
        let factory = ChatIconViewModelFactory(
            productFactory: iconFactory,
            publicProfileFactory: publicProfileFactory,
            options: .init(forceFresh: false, skipStale: false)
        )

        #expect(factory.createViewModel(from: .product(domain: productId)) != nil)
        #expect(factory.createViewModel(from: .bot) == nil)
        #expect(iconFactory.requestedDomains == [productId])
    }
}

private extension ProductBotChatListTests {
    func makeBot(description: String?) -> ProductBot {
        ProductBot(
            product: Product(id: productId, name: "Jollity"),
            description: description,
            runtime: IdleChatRuntime(),
            resolveImage: WidgetImageResolver { _ in nil }
        )
    }

    func chatListPreview(alt: String?) throws -> String? {
        let bot = makeBot(description: "Play the weekly game with friends")
        let chatId = Chat.Id.chatExtension(bot.product.id.rawValue)
        let message = Chat.LocalMessage(
            messageId: "results",
            chatId: chatId,
            origin: .chatExtension(bot.product.id.rawValue),
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
            peer: .chatExtension(bot.product.id.rawValue),
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
            iconViewModelFactory: ChatIconViewModelFactory(
                productFactory: RecordingProductIconViewModelFactory(),
                publicProfileFactory: UnusedPublicProfileAvatarFactory(),
                options: .init(forceFresh: false, skipStale: false)
            ),
            chain: chain,
            tokenFormatter: { info in
                TransferAmountViewModelFactory(targetAssetInfo: info, formatterFactory: AssetBalanceFormatterFactory())
            }
        )

        let viewModel = factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(
                establishedChats: [ChatWithPeerMetadata(chat: chat, peerMetadata: bot.peerMetadata.list)],
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
    private(set) var requestedDomains: [ProductId] = []

    func createViewModel(for domain: ProductId) -> any IdentifiableImageViewModelProtocol {
        requestedDomains.append(domain)
        return EmptyProductIconViewModel(identifier: domain.rawValue)
    }
}

private struct UnusedPublicProfileAvatarFactory: PublicProfileAvatarViewModelMaking {
    func createViewModel(
        for _: String,
        options _: PublicProfileResolutionOptions
    ) -> any IdentifiableImageViewModelProtocol {
        Issue.record("Product icons must not resolve public profiles")
        return EmptyProductIconViewModel(identifier: "unused")
    }
}

private struct EmptyProductIconViewModel: IdentifiableImageViewModelProtocol {
    let identifier: String

    @MainActor
    func loadImage(
        on _: UIImageView,
        settings _: ImageViewModelSettings,
        animated _: Bool,
        completion: ((Bool) -> Void)?
    ) {
        completion?(false)
    }

    @MainActor
    func cancel(on _: UIImageView) {}
}
