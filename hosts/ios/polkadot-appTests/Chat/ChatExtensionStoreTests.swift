import AsyncExtensions
import Foundation
import Keystore_iOS
import PolkadotUI
import Products
import StructuredConcurrency
import Testing
@testable import polkadot_app

/// A product's rooms are listed while the product is installed or host-placed. Its bot arrives
/// only after the manifest resolves and the worker archive downloads, so until then the list
/// draws the row from the product itself.
@Suite("Product chat rows before the bot is built")
struct ChatExtensionStoreTests {
    private let product = Product(id: "dim2.paseo", name: "Jollity")
    private let provider = StubProductBotProvider()

    /// A cold start paints the list before any manifest is read; the row must not wait for the bot.
    @Test
    @MainActor
    func aListedProductHasARowBeforeItsBotIsBuilt() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)

        provider.emit(ProductBots(products: [product], built: []))

        #expect(try await changes.next() == .enabled([product.extensionId]))
        #expect(registry.hasChatExtension(for: product.extensionId))
        #expect(registry.getChatExtensionBot(for: product.extensionId) == nil)
        #expect(registry.peerMetadata(for: product.extensionId) == Chat.PeerMetadata(
            name: "Jollity",
            icon: .product(domain: "dim2.paseo"),
            input: .empty,
            moreActions: []
        ))
    }

    @Test
    @MainActor
    func theBotTakesOverItsProductRowOnceBuilt() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)
        let bot = makeBot()

        provider.emit(ProductBots(products: [product], built: []))
        _ = try await changes.next()
        provider.emit(ProductBots(products: [product], built: [bot]))

        #expect(try await changes.next() == .enabled([product.extensionId]))
        #expect(registry.getChatExtensionBot(for: product.extensionId) === bot)
        #expect(registry.peerMetadata(for: product.extensionId) == bot.peerMetadata)
    }

    @Test
    @MainActor
    func anUninstalledProductLosesItsRow() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)

        provider.emit(ProductBots(products: [product], built: [makeBot()]))
        _ = try await changes.next()
        provider.emit(ProductBots(products: [], built: []))

        #expect(try await changes.next() == .disabled([product.extensionId]))
        #expect(!registry.hasChatExtension(for: product.extensionId))
        #expect(registry.peerMetadata(for: product.extensionId) == nil)
    }
}

private extension ChatExtensionStoreTests {
    @MainActor
    func makeRegistry() -> ChatExtensionsRegistering {
        let registry = ChatExtensionsRegistry(
            extensionStore: ChatExtensionStore(staticExtensions: [], productBotProvider: provider),
            storageFacade: UserDataStorageTestFacade(),
            settingsManager: InMemorySettingsManager()
        )
        registry.discover()
        return registry
    }

    func makeBot() -> ProductBot {
        ProductBot(
            product: product,
            description: nil,
            runtime: IdleChatRuntime(),
            resolveImage: WidgetImageResolver { _ in nil }
        )
    }
}

/// Subscribes before the test emits, so no change is missed, and bounds each wait so a store that
/// never reports one fails instead of hanging the suite.
private final class ChangeFeed: @unchecked Sendable {
    private var changes: AnyAsyncIterator<ChatExtensionRegistryChange>

    init(_ registry: ChatExtensionsRegistering) {
        changes = registry.onChangeStream.makeAsyncIterator()
    }

    func next() async throws -> ChatExtensionRegistryChange? {
        try await withTimeout(.seconds(5)) { [self] in try await changes.next() }
    }
}
