import ChainRegistry
import Foundation
import Operation_iOS
@testable import PolkadotUI
import Products
import Testing
import UIKit
@testable import polkadot_app

/// The text of product bodies (02-DESIGN §6.1; PARITY rows 3 and 5).
@MainActor
struct ChatProductBodyRenderingTests {
    private typealias Data = ProductRoomTestData

    private let doubles = ChatRoomAdditionsDoubles()
    private let wireframe = RecordingProductRoomWireframe()

    // MARK: - Text (O18)

    /// A `Text` body of a product is plain: no Markdown and no links, also for a bare address.
    @Test func aProductTextIsPlainWithNoLinks() async throws {
        let raw = "**not bold** [x](https://example.com) and https://example.com/a"
        let (factory, presenter) = try makeFactory()
        defer { withExtendedLifetime(presenter) {} }

        let text = try await textViewModel(factory, Data.message("m1", content: .text(raw)))

        #expect(text.rawText == raw)
        #expect(String(text.attributedText.characters) == raw)
        #expect(text.attributedText.runs.allSatisfy { $0.link == nil })
    }

    /// The text of the user in a product room is a `Text` body too.
    @Test func theOwnTextOfTheUserInAProductRoomIsPlain() async throws {
        let (factory, presenter) = try makeFactory()
        defer { withExtendedLifetime(presenter) {} }
        let own = Data.message("m1", content: .text("*mine*"), origin: .user, status: .outgoing(.delivered))

        let text = try await textViewModel(factory, own)

        #expect(String(text.attributedText.characters) == "*mine*")
    }

    /// A person chat keeps inline Markdown (O18).
    @Test func aPersonChatKeepsInlineMarkdown() async throws {
        let (factory, presenter) = try makeFactory()
        defer { withExtendedLifetime(presenter) {} }
        factory.productRoom = nil

        let text = try await textViewModel(factory, Data.message("m1", content: .text("**bold**")))

        #expect(String(text.attributedText.characters) == "bold")
    }

    /// A `RichText` body keeps its `https` links, which open through the host after a confirmation.
    @Test func aRichTextKeepsItsHttpsLinkAndOpensItThroughTheHost() async throws {
        let (factory, presenter) = try makeFactory()
        defer { withExtendedLifetime(presenter) {} }
        let body = Chat.LocalMessage.Content.ProductBody.richText(.init(text: "[x](https://example.com)", media: []))

        let text = try await textViewModel(factory, Data.message("m1", content: .productBody(body)))

        #expect(text.attributedText.runs.contains { $0.link == URL(string: "https://example.com") })
        #expect(text.openLink != nil)
    }

    // MARK: - Show more (§6.1)

    @Test func aLongTextShowsItsStartUntilTheUserExpandsIt() async throws {
        let (factory, presenter) = try makeFactory()
        let long = String(repeating: "a", count: ChatTextCollapse.collapsedLength + 100)
        let message = Data.message("m1", content: .text(long))

        let collapsed = try await textViewModel(factory, message)
        #expect(collapsed.isCollapsed)
        #expect(collapsed.shownText.count == ChatTextCollapse.collapsedLength + 1)
        #expect(collapsed.shownText.hasSuffix("…"))

        presenter.expandText(of: "m1")
        let expanded = try await textViewModel(factory, message)

        #expect(!expanded.isCollapsed)
        #expect(expanded.shownText == long)
    }

    @Test func aShortTextAndALongRichTextFollowTheBound() async throws {
        let (factory, presenter) = try makeFactory()
        defer { withExtendedLifetime(presenter) {} }
        let long = String(repeating: "b", count: ChatTextCollapse.collapsedLength + 1)
        let rich = Chat.LocalMessage.Content.ProductBody.richText(.init(text: long, media: []))

        let short = try await textViewModel(factory, Data.message("m1", content: .text("short")))
        let richText = try await textViewModel(factory, Data.message("m2", content: .productBody(rich)))

        #expect(!short.isCollapsed)
        #expect(richText.isCollapsed)
    }

    @Test func theCollapseCutsTheTextAtTheBound() {
        let text = AttributedString(String(repeating: "c", count: 10))

        #expect(ChatTextCollapse.collapsed(text, length: 10) == nil)
        #expect(ChatTextCollapse.collapsed(text, length: 4).map { String($0.characters) } == "cccc…")
    }
}

// MARK: - Helpers

private extension ChatProductBodyRenderingTests {
    func makeFactory(
        resolver: ChatResourceResolving = NoImageResolver()
    ) throws -> (ChatViewModelFactory, ChatProductRoomPresenter) {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: RecordingProductRoom(),
            wireframe: wireframe,
            resolver: resolver,
            activityStore: ChatRoomActivityStore(),
            additions: doubles.dependencies(),
            logger: StubLogger()
        ))
        presenter.update(policy: ChatRoomPolicy(footer: nil, declarations: Data.declarations()), messages: [])

        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "Polkadot"), order: 0)
        let asset = try #require(chain.assets.first)
        let dotNs = StubDotNsResolver()
        let factory = try ChatViewModelFactory(
            balanceFactory: TransferAmountViewModelFactory(
                targetAssetInfo: asset.displayInfo,
                formatterFactory: AssetBalanceFormatterFactory()
            ),
            customDecoders: [],
            attachmentViewModelFactory: NoProductAttachmentItems(),
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

    func bubble(
        _ factory: ChatViewModelFactory,
        _ message: Chat.LocalMessage
    ) async throws -> ChatMessageContainerConfiguration {
        let listModel = await ChatInteractorState().onMessagesUpdate([message])
        let chatMetadata = ChatMetadata(
            chatId: Data.chatId,
            peerMetadata: Chat.PeerMetadata(
                name: "Game",
                icon: .bot,
                input: .inputField(.init(canPay: false, canAttachFile: false)),
                moreActions: []
            ),
            state: .created,
            roomPolicy: factory.productRoom?.policy ?? .defaults
        )

        let viewModel = factory.viewModel(
            model: listModel,
            metadata: MessageListMetadata(chatMetadata: chatMetadata, myUsername: "me"),
            actions: .silent,
            footerConfiguration: nil
        )
        let item = try #require(viewModel.sections.flatMap(\.messages).first { $0.id == message.messageId })
        return try #require(item.configuration as? ChatMessageContainerConfiguration)
    }

    func textViewModel(
        _ factory: ChatViewModelFactory,
        _ message: Chat.LocalMessage
    ) async throws -> ChatMessageTextView.ViewModel {
        let bubble = try await bubble(factory, message)
        let content = try #require(bubble.innerContentConfiguration as? ChatRichTextMessageConfiguration)
        return try #require(content.textViewModel)
    }
}

private struct NoProductAttachmentItems: ChatAttachmentViewModelMaking {
    func makeAttachmentItem(
        for _: Chat.LocalMessage.Content.Attachment,
        messageId _: Chat.MessageId,
        onSelection _: @escaping () -> Void
    ) -> ChatRichTextMessageConfiguration.AttachmentItem? {
        nil
    }
}
