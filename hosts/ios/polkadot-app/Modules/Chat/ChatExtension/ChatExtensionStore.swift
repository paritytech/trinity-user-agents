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
                for try await event in productBotProvider.observeBots() {
                    guard !Task.isCancelled else { return }
                    self?.apply(event)
                }
            } catch {
                // Stream completed with error — observation stops
            }
        }
    }

    // MARK: - Private

    private func apply(_ event: ProductBotEvent) {
        switch event {
        case let .listed(products):
            apply(listed: products)
        case let .botBuilt(bot):
            apply(built: bot)
        }
    }

    /// A product is enabled when it is listed, so the list draws its rows at once, and loses its
    /// rows and its bot when it leaves.
    private func apply(listed incoming: [Product]) {
        let incomingProducts = Dictionary(
            incoming.map { ($0.extensionId, $0) },
            uniquingKeysWith: { _, last in last }
        )

        lock.lock()

        let removedIds = Set(products.keys).subtracting(incomingProducts.keys)
        let addedIds = Set(incomingProducts.keys).subtracting(products.keys)
        products = incomingProducts

        let removedBots = removedIds.compactMap { productBots.removeValue(forKey: $0) }

        lock.unlock()

        for bot in removedBots {
            Task { await bot.dispose() }
        }

        if !addedIds.isEmpty {
            delegate?.didEnableExtensions(addedIds)
        }

        if !removedIds.isEmpty {
            delegate?.didDisableExtensions(removedIds)
        }
    }

    /// The bot takes its product's rows over, so the product is enabled again and the list
    /// redraws them with the bot's metadata. A product keeps the bot it already has, since
    /// swapping one would drop the worker it drives, and a bot built for a product that left
    /// the list is dropped.
    private func apply(built bot: ProductBot) {
        lock.lock()

        let takesOver = products[bot.identifier] != nil && productBots[bot.identifier] == nil

        if takesOver {
            productBots[bot.identifier] = bot
        }

        lock.unlock()

        guard takesOver else {
            Task { await bot.dispose() }
            return
        }

        delegate?.didEnableExtensions([bot.identifier])
    }
}
