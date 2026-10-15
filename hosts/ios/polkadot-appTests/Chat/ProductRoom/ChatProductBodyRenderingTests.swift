import ChainRegistry
import Foundation
import Operation_iOS
@testable import PolkadotUI
import Products
import SubstrateSdk
import Testing
import UIKit
@testable import polkadot_app

/// The text and the media of product bodies (02-DESIGN §6.1, §6.2; PARITY rows 3, 4 and 5).
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

    // MARK: - Media (§6.2)

    @Test func theMediaBubbleKnowsEachKindAndOpensThem() async throws {
        let (factory, presenter) = try makeFactory()
        defer { withExtendedLifetime(presenter) {} }
        let body = Chat.LocalMessage.Content.ProductBody.richText(.init(
            text: nil,
            media: ["data:image/png;base64,AA==", "preimage:ab", "preimage:cd"],
            items: [
                .init(kind: .image, mimeType: "image/png", fileName: nil, sizeBytes: nil),
                .init(kind: .video, mimeType: "video/mp4", fileName: nil, sizeBytes: nil),
                .init(kind: .file, mimeType: "application/pdf", fileName: "r.pdf", sizeBytes: 2_048)
            ]
        ))

        let stack = try await mediaStack(factory, Data.message("m1", content: .productBody(body)))

        #expect(stack.items.map(\.kind) == [.image, .video, .file(name: "r.pdf", detail: "2 KB")])
        #expect(stack.opening != nil)
    }

    /// A body that a host stored before it kept the kinds: an inline video is a video, the rest are images.
    @Test func aStoredBodyWithoutKindsReadsTheKindFromTheSource() {
        let richText = Chat.LocalMessage.Content.ProductRichText(
            text: nil,
            media: ["data:video/mp4;base64,AA==", "https://example.com/a.png"]
        )

        #expect(richText.info(at: 0).kind == .video)
        #expect(richText.info(at: 1).kind == .image)
    }

    @Test func theKindsAreStoredWithTheBodyAndAnOlderRowDecodesWithout() throws {
        let richText = Chat.LocalMessage.Content.ProductRichText(
            text: "hi",
            media: ["preimage:ab"],
            alts: ["A film"],
            items: [.init(kind: .video, mimeType: "video/mp4", fileName: nil, sizeBytes: 9)]
        )
        let encoder = ScaleEncoder()
        try richText.encode(scaleEncoder: encoder)
        let decoded = try Chat.LocalMessage.Content.ProductRichText(scaleDecoder: ScaleDecoder(data: encoder.encode()))
        #expect(decoded == richText)

        let older = ScaleEncoder()
        try ScaleOption(value: "hi").encode(scaleEncoder: older)
        try ["preimage:ab"].encode(scaleEncoder: older)
        try [ScaleOption(value: "A film")].encode(scaleEncoder: older)
        let olderRow = try Chat.LocalMessage.Content.ProductRichText(scaleDecoder: ScaleDecoder(data: older.encode()))
        #expect(olderRow.items.isEmpty)
        #expect(olderRow.alts == ["A film"])
    }

    @Test func anOpenedImageShowsFullScreenFromALocalFile() async throws {
        let png = try #require(UIGraphicsImageRenderer(size: CGSize(width: 2, height: 2)).image { _ in }.pngData())
        let (_, presenter) = try makeFactory(resolver: FixedDataResolver(data: png))
        let body = Chat.LocalMessage.Content.ProductBody.richText(.init(text: nil, media: ["preimage:ab"]))
        presenter.update(policy: presenter.policy, messages: [Data.message("m1", content: .productBody(body))])

        let request = try #require(presenter.mediaRequest(of: "m1", at: 0))
        let opened = await presenter.openMedia(request)

        #expect(opened)
        #expect(wireframe.shownMedia.map(\.kind) == [.image])
        let url = try #require(wireframe.shownMedia.first?.url)
        #expect(url.pathExtension == "png")
        #expect(try Foundation.Data(contentsOf: url) == png)
    }

    /// A file that the product keeps inline or as a preimage opens in the document preview with its name.
    @Test func aFileOpensInTheDocumentPreviewWithItsName() async throws {
        let bytes = Foundation.Data("%PDF".utf8)
        let (_, presenter) = try makeFactory(resolver: FixedDataResolver(data: bytes))
        let file = Chat.LocalMessage.Content.ProductBody.file(.init(
            url: "preimage:ab",
            fileName: "../report.pdf",
            mimeType: "application/pdf",
            sizeBytes: 4,
            text: nil
        ))
        presenter.update(policy: presenter.policy, messages: [Data.message("m1", content: .productBody(file))])

        let request = try #require(presenter.mediaRequest(of: "m1", at: 0))
        #expect(await presenter.openMedia(request))

        #expect(wireframe.shownMedia.map(\.kind) == [.file])
        #expect(wireframe.shownMedia.first?.url.lastPathComponent == ".._report.pdf")
    }

    /// A missing preimage opens nothing, so the bubble offers Retry.
    @Test func aMissingPreimageOpensNothing() async throws {
        let (_, presenter) = try makeFactory()
        let body = Chat.LocalMessage.Content.ProductBody.richText(.init(text: nil, media: ["preimage:ab"]))
        presenter.update(policy: presenter.policy, messages: [Data.message("m1", content: .productBody(body))])

        let request = try #require(presenter.mediaRequest(of: "m1", at: 0))

        #expect(await presenter.openMedia(request) == false)
        #expect(wireframe.shownMedia.isEmpty)
    }

    @Test func theLocalFileOfAnImageAndAVideoGetsTheExtensionOfItsType() {
        let video = Chat.LocalMessage.Content.ProductMediaInfo(
            kind: .video,
            mimeType: "video/mp4",
            fileName: nil,
            sizeBytes: nil
        )
        let unknown = Chat.LocalMessage.Content.ProductMediaInfo(
            kind: .video,
            mimeType: "",
            fileName: nil,
            sizeBytes: nil
        )

        #expect(ProductMediaFiles.fileName(of: video, data: Foundation.Data()) == "media.mp4")
        #expect(ProductMediaFiles.fileName(of: unknown, data: Foundation.Data()) == "media.mp4")
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

    func mediaStack(_ factory: ChatViewModelFactory, _ message: Chat.LocalMessage) async throws -> ChatMediaStackView {
        let bubble = try await bubble(factory, message)
        let content = try #require(bubble.innerContentConfiguration as? SwiftUIContentConfiguration)
        return try #require(Mirror(reflecting: content).descendant("provider", "view") as? ChatMediaStackView)
    }
}

/// A resolver that gives the same bytes for each resource.
private struct FixedDataResolver: ChatResourceResolving {
    let data: Foundation.Data

    func data(for _: polkadot_app.ChatResource) async -> Foundation.Data? { data }
    func image(for _: polkadot_app.ChatResource) async -> UIImage? { UIImage(data: data) }
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
