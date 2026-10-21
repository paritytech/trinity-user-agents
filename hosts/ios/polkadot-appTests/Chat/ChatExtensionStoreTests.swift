import AsyncExtensions
import Foundation
import Keystore_iOS
import PolkadotUI
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
@testable import polkadot_app

/// A product's rooms are listed while the product is installed or host-placed. Its bot arrives
/// only after the manifest resolves and the worker archive downloads, so until then the list
/// draws the row from the product itself, and each bot takes its row over as soon as it is built.
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

        provider.emit(.listed([product]))

        #expect(try await changes.next() == .enabled([product.extensionId]))
        #expect(registry.hasChatExtension(for: product.extensionId))
        #expect(registry.getChatExtensionBot(for: product.extensionId) == nil)
        #expect(registry.peerListMetadata(for: product.extensionId) == Chat.PeerListMetadata(
            name: "Jollity",
            icon: .product(domain: "dim2.paseo")
        ))
    }

    @Test
    @MainActor
    func theBotTakesOverItsProductRowOnceBuilt() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)
        let bot = makeBot()

        provider.emit(.listed([product]))
        _ = try await changes.next()
        provider.emit(.botBuilt(bot))

        #expect(try await changes.next() == .enabled([product.extensionId]))
        #expect(registry.getChatExtensionBot(for: product.extensionId) === bot)
        #expect(registry.peerListMetadata(for: product.extensionId) == bot.peerMetadata.list)
    }

    @Test
    @MainActor
    func anUninstalledProductLosesItsRow() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)

        provider.emit(.listed([product]))
        _ = try await changes.next()
        provider.emit(.botBuilt(makeBot()))
        _ = try await changes.next()
        provider.emit(.listed([]))

        #expect(try await changes.next() == .disabled([product.extensionId]))
        #expect(!registry.hasChatExtension(for: product.extensionId))
        #expect(registry.peerListMetadata(for: product.extensionId) == nil)
    }

    /// A build that finishes after its product left the list must not resurrect the row.
    @Test
    @MainActor
    func aBotBuiltForAProductNoLongerListedIsDropped() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)

        provider.emit(.listed([product]))
        _ = try await changes.next()
        provider.emit(.listed([]))
        _ = try await changes.next()
        provider.emit(.botBuilt(makeBot()))
        provider.emit(.listed([Product(id: "other.paseo", name: "Other")]))

        #expect(try await changes.next() == .enabled(["other.paseo"]))
        #expect(registry.getChatExtensionBot(for: product.extensionId) == nil)
        #expect(!registry.hasChatExtension(for: product.extensionId))
    }

    /// A row is on the list before its bot exists, so a tap must wait for the bot rather than
    /// open a chat with nothing behind it.
    @Test
    @MainActor
    func aTapOnAListedProductWaitsForItsBot() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)
        let model = ChatOpenModel.existingChat(.chatExtension(product.extensionId, roomId: "jollity"))

        provider.emit(.listed([product]))
        _ = try await changes.next()

        let opening = Task { try await registry.entryRoute(for: model) }
        provider.emit(.botBuilt(makeBot()))

        guard case let .chat(.existingChat(routedId)) = try await opening.value else {
            Issue.record("the tap must route into the chat once the bot is built")
            return
        }
        #expect(routedId == .chatExtension(product.extensionId, roomId: "jollity"))
    }

    /// A broken archive, or any other reason the bot never comes, must end in an error the user
    /// can read, not in a chat that waits forever. The ten seconds pass on a manual clock, so the
    /// test neither waits for them nor depends on how loaded the machine is.
    @Test
    @MainActor
    func aTapGivesUpWhenTheBotDoesNotComeInTime() async throws {
        let registry = makeRegistry()
        let changes = ChangeFeed(registry)
        let model = ChatOpenModel.existingChat(.chatExtension(product.extensionId, roomId: "jollity"))
        let clock = ManualClock()

        provider.emit(.listed([product]))
        _ = try await changes.next()

        let opening = Task { try await registry.entryRoute(for: model, clock: clock) }
        // Bounded, so a tap that stopped waiting fails the test instead of hanging the suite.
        try await withTimeout(.seconds(5)) { await clock.resumeSleep(for: .seconds(10)) }

        await #expect(throws: ChatExtensionBotError.self) {
            try await opening.value
        }
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
