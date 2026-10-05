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
                    await self?.diffAndApply(incomingBots)
                }
            } catch {
                // Stream completed with error — observation stops
            }
        }
    }

    // MARK: - Private

    private func diffAndApply(_ incomingBots: [ProductBot]) async {
        let incomingById = Dictionary(
            incomingBots.map { ($0.identifier, $0) },
            uniquingKeysWith: { _, last in last }
        )

        let (addedIds, removedIds, removed) = lock.withLock {
            let existingIds = Set(productBots.keys)
            let incomingIds = Set(incomingById.keys)
            let replacedIds = existingIds.intersection(incomingIds).filter {
                productBots[$0]?.runtimeOwner != incomingById[$0]?.runtimeOwner
            }
            let addedIds = incomingIds.subtracting(existingIds).union(replacedIds)
            let removedIds = existingIds.subtracting(incomingIds).union(replacedIds)
            let removed = removedIds.compactMap { productBots.removeValue(forKey: $0) }

            for identifier in addedIds {
                productBots[identifier] = incomingById[identifier]
            }
            return (addedIds, removedIds, removed)
        }

        for bot in removed {
            await bot.dispose()
        }

        if !removedIds.isEmpty {
            delegate?.didDisableExtensions(removedIds)
        }

        if !addedIds.isEmpty {
            delegate?.didEnableExtensions(addedIds)
        }
    }
}
