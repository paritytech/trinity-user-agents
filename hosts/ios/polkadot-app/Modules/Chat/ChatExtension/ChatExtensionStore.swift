import Foundation
import AsyncExtensions

protocol ChatExtensionStoring: AnyObject {
    var delegate: ChatExtensionDelegate? { get set }

    var allExtensions: [ChatExtending] { get }

    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol?

    func startObserving()
}

final class ChatExtensionStore: ChatExtensionStoring {
    weak var delegate: ChatExtensionDelegate?

    private let staticExtensions: [ChatExtending]
    private let productBotProvider: ProductBotProviding

    private let lock = NSLock()
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

    func startObserving() {
        observationTask = Task { [weak self, productBotProvider] in
            do {
                for try await incomingBots in productBotProvider.observeBots() {
                    guard !Task.isCancelled else { return }
                    self?.diffAndApply(incomingBots)
                }
            } catch {
                // Stream completed with error — observation stops
            }
        }
    }

    // MARK: - Private

    private func diffAndApply(_ incomingBots: [ProductBot]) {
        let incomingById = Dictionary(
            incomingBots.map { ($0.identifier, $0) },
            uniquingKeysWith: { _, last in last }
        )

        let added: [ProductBot]
        let removed: [ProductBot]

        lock.lock()

        let existingIds = Set(productBots.keys)
        let incomingIds = Set(incomingById.keys)

        let addedIds = incomingIds.subtracting(existingIds)
        let removedIds = existingIds.subtracting(incomingIds)

        removed = removedIds.compactMap { productBots.removeValue(forKey: $0) }
        added = addedIds.compactMap { incomingById[$0] }

        for bot in added {
            productBots[bot.identifier] = bot
        }

        lock.unlock()

        for bot in removed {
            Task { await bot.dispose() }
        }

        if !addedIds.isEmpty {
            delegate?.didEnableExtensions(addedIds)
        }

        if !removedIds.isEmpty {
            delegate?.didDisableExtensions(removedIds)
        }
    }
}
