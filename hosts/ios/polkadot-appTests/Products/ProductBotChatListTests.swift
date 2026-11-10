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
    func eachChatHeaderRequestsFreshMetadataOnce() throws {
        let profiles = RecordingPublicProfileAvatarViewModelFactory()
        let header = ChatHeaderIconViewModelFactory(
            productFactory: RecordingProductIconViewModelFactory(),
            publicProfileFactory: profiles
        )
        let icon = Chat.PeerMetadata.Icon.publicProfile(username: "alice.01")
        let initial = try #require(header.viewModel(for: icon))
        let redrawn = try #require(header.viewModel(for: icon))
        let reopened = ChatHeaderIconViewModelFactory(
            productFactory: RecordingProductIconViewModelFactory(),
            publicProfileFactory: profiles
        )
        _ = reopened.viewModel(for: icon)

        #expect(initial === redrawn)
        #expect(profiles.requests == [
            .init(username: "alice.01", options: .init(forceFresh: true, skipStale: false)),
            .init(username: "alice.01", options: .init(forceFresh: true, skipStale: false))
        ])
    }

    @Test
    func chatListAvatarsRespectTheNormalFreshnessPolicy() throws {
        let profiles = RecordingPublicProfileAvatarViewModelFactory()
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let factory = ContactsListViewModelFactory(
            chatMessageDecoderFactory: FixedChatMessageDecoderFactory(decoders: []),
            productIconViewModelFactory: RecordingProductIconViewModelFactory(),
            publicProfileAvatarViewModelFactory: profiles,
            chain: chain,
            tokenFormatter: { info in
                TransferAmountViewModelFactory(targetAssetInfo: info, formatterFactory: AssetBalanceFormatterFactory())
            }
        )
        let chat = Chat.LocalModel(
            peer: .person(.init(
                accountId: Data(repeating: 1, count: 32),
                username: "alice.01",
                publicKey: Data(repeating: 2, count: 32),
                ownKeyId: .init(signKeyId: "signing", encryptionKeyId: "encryption"),
                imageData: nil,
                isBlocked: false,
                devices: [],
                pendingDevicesFanOut: false,
                addedAt: nil,
                acceptedAt: nil
            )),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil
        )

        _ = factory.createViewModel(
            assetDisplayInfo: asset.displayInfo,
            model: ChatListModel(
                establishedChats: [.init(
                    chat: chat,
                    peerMetadata: .init(name: "Alice", icon: .publicProfile(username: "alice.01"))
                )],
                pendingIncomingRequestCount: 0,
                newIncomingRequestCount: 0
            )
        )

        #expect(profiles.requests == [.init(username: "alice.01", options: .init(forceFresh: false, skipStale: false))])
    }

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
        let publicProfileFactory = PublicProfileAvatarViewModelFactory(profiles: { _, _ in
            AsyncThrowingStream { $0.finish() }
        }, imageLoader: CIDImageLoader(loadImage: { _ in
            throw ImageError.Unavailable(reason: "no profile avatar")
        }))

        #expect(Chat.PeerMetadata.Icon.product(domain: productId).imageViewModel(
            using: iconFactory,
            publicProfileFactory: publicProfileFactory,
            options: .init(forceFresh: false, skipStale: false)
        ) != nil)
        #expect(Chat.PeerMetadata.Icon.bot.imageViewModel(
            using: iconFactory,
            publicProfileFactory: publicProfileFactory,
            options: .init(forceFresh: false, skipStale: false)
        ) == nil)
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
            productIconViewModelFactory: RecordingProductIconViewModelFactory(),
            publicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelFactory(profiles: { _, _ in
                AsyncThrowingStream { $0.finish() }
            }, imageLoader: CIDImageLoader(loadImage: { _ in
                throw ImageError.Unavailable(reason: "no profile avatar")
            })),
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

private final class RecordingPublicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelMaking {
    struct Request: Equatable {
        let username: String
        let options: PublicProfileResolutionOptions
    }

    private(set) var requests: [Request] = []

    func createViewModel(
        for username: String,
        options: PublicProfileResolutionOptions
    ) -> any IdentifiableImageViewModelProtocol {
        requests.append(.init(username: username, options: options))
        return EmptyProductIconViewModel(identifier: "profile:\(username)")
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
