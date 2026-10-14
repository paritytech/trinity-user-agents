import Foundation
import AsyncExtensions
import Products

protocol ChatExtensionStoring: AnyObject {
    var delegate: ChatExtensionDelegate? { get set }

    var allExtensions: [ChatExtending] { get }

    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol?

    /// The product listed under this id, whether or not its bot is built yet.
    func product(for extensionId: ChatExtension.Id) -> Product?

    func startObserving()
}

final class ChatExtensionStore: ChatExtensionStoring {
    weak var delegate: ChatExtensionDelegate?

    private let staticExtensions: [ChatExtending]
    private let productBotProvider: ProductBotProviding

    private let lock = NSLock()
    private var products: [ChatExtension.Id: Product] = [:]
    private var productBots: [ChatExtension.Id: ProductBot] = [:]
    private var observationTask: Task<Void, Never>?

    init(
        staticExtensions: [ChatExtending],
        productBotProvider: ProductBotProviding
    ) {
        self.staticExtensions = staticExtensions
        self.productBotProvider = productBotProvider
    }

    deinit {
        observationTask?.cancel()
    }

    // MARK: - ChatExtensionStoring

    var allExtensions: [ChatExtending] {
        lock.lock()
        let bots = staticExtensions + Array(productBots.values)
        lock.unlock()

        return bots
    }

    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol? {
        for ext in staticExtensions {
            if ext.identifier == extensionId, let bot = ext as? ChatExtensionBotProtocol {
                return bot
            }
        }

        lock.lock()
        let bot = productBots[extensionId]
        lock.unlock()
        return bot
    }

    func product(for extensionId: ChatExtension.Id) -> Product? {
        lock.lock()
        let product = products[extensionId]
        lock.unlock()
        return product
    }

    func startObserving() {
        observationTask = Task { [weak self, productBotProvider] in
            do {
                for try await bots in productBotProvider.observeBots() {
                    guard !Task.isCancelled else { return }
                    self?.apply(bots)
                }
            } catch {
                // Stream completed with error — observation stops
            }
        }
    }

    // MARK: - Private

    /// A product keeps the bot it already has: swapping one would drop the worker it drives, and a
    /// later build that yields none leaves it in place; only a product that left the list loses
    /// its bot. A product is enabled when it is listed and again when its bot is built, so the
    /// list redraws its rows with the bot's metadata.
    private func apply(_ incoming: ProductBots) {
        let incomingProducts = Dictionary(
            incoming.products.map { ($0.extensionId, $0) },
            uniquingKeysWith: { _, last in last }
        )
        let incomingBots = Dictionary(
            incoming.built.map { ($0.identifier, $0) },
            uniquingKeysWith: { _, last in last }
        )

        lock.lock()

        let removedIds = Set(products.keys).subtracting(incomingProducts.keys)
        let addedProductIds = Set(incomingProducts.keys).subtracting(products.keys)
        products = incomingProducts

        let removedBots = removedIds.compactMap { productBots.removeValue(forKey: $0) }
        let addedBotIds = Set(incomingBots.keys).subtracting(productBots.keys)

        for id in addedBotIds {
            productBots[id] = incomingBots[id]
        }

        lock.unlock()

        for bot in removedBots {
            Task { await bot.dispose() }
        }

        let enabledIds = addedProductIds.union(addedBotIds)

        if !enabledIds.isEmpty {
            delegate?.didEnableExtensions(enabledIds)
        }

        if !removedIds.isEmpty {
            delegate?.didDisableExtensions(removedIds)
        }
    }
}
