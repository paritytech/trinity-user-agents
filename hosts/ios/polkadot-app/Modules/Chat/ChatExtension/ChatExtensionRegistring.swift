import Foundation
import AsyncExtensions
import Keystore_iOS
import StructuredConcurrency

protocol ChatExtensionsRegistering {
    var onChangeStream: AnyAsyncSequence<ChatExtensionRegistryChange> { get }

    func discover()

    func getExtensions(for chatId: Chat.Id) -> [ChatExtending]
    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol?

    /// How the chat list draws the extension's rooms: its bot's name and icon, or the product's
    /// own while the bot is not built. Nil for an extension the host does not know.
    func peerListMetadata(for extensionId: ChatExtension.Id) -> Chat.PeerListMetadata?

    func getWidgetProviders() -> [ChatExtensionWidgetProvider]
}

extension ChatExtensionsRegistering {
    func hasChatExtension(for extensionId: ChatExtension.Id) -> Bool {
        peerListMetadata(for: extensionId) != nil
    }

    func getOwningExtension(for model: ChatOpenModel) -> ChatExtending? {
        guard
            case let .existingChat(chatId) = model,
            case let .chatExtension(extensionId, _) = chatId
        else {
            return nil
        }

        return getChatExtensionBot(for: extensionId)
    }

    /// A listed product's row can be tapped before its bot is built, so the tap waits for the bot
    /// up to ten seconds on `clock` rather than open a chat with nothing behind it, and gives up
    /// with ``ChatExtensionBotError/notReady(_:)`` when the bot does not come, as with a broken
    /// archive.
    func entryRoute(
        for model: ChatOpenModel,
        clock: any Clock<Duration> = ContinuousClock()
    ) async throws -> ChatExtensionEntryRoute {
        if let chatExtension = getOwningExtension(for: model) {
            return await chatExtension.entryRoute(for: model)
        }

        guard
            case let .existingChat(chatId) = model,
            case let .chatExtension(extensionId, _) = chatId,
            hasChatExtension(for: extensionId)
        else {
            return .chat(model)
        }

        let bot = try await awaitChatExtensionBot(for: extensionId, clock: clock)

        return await bot.entryRoute(for: model)
    }

    private func awaitChatExtensionBot(
        for extensionId: ChatExtension.Id,
        clock: any Clock<Duration>
    ) async throws -> ChatExtensionBotProtocol {
        do {
            return try await withTimeout(.seconds(10), clock: clock) {
                // Subscribed before the lookup, so a bot built in between is not missed.
                var changes = onChangeStream.makeAsyncIterator()

                if let bot = getChatExtensionBot(for: extensionId) {
                    return bot
                }

                while let change = try await changes.next() {
                    if case let .enabled(ids) = change, ids.contains(extensionId),
                       let bot = getChatExtensionBot(for: extensionId) {
                        return bot
                    }
                }

                throw ChatExtensionBotError.notReady(extensionId)
            }
        } catch is TimeoutError {
            throw ChatExtensionBotError.notReady(extensionId)
        }
    }
}

enum ChatExtensionRegistryChange: Equatable {
    case enabled(Set<ChatExtension.Id>)
    case disabled(Set<ChatExtension.Id>)
}

final class ChatExtensionsRegistry {
    let extensionStore: ChatExtensionStoring
    let settingsManager: SettingsManagerProtocol & ChatExtensionBotSettings
    let storageFacade: StorageFacadeProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    private let changeSubject = AsyncPassthroughSubject<ChatExtensionRegistryChange>()

    @MainActor
    init(
        extensionStore: ChatExtensionStoring,
        storageFacade: StorageFacadeProtocol,
        settingsManager: SettingsManagerProtocol & ChatExtensionBotSettings,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.extensionStore = extensionStore
        self.settingsManager = settingsManager
        self.storageFacade = storageFacade
        self.operationQueue = operationQueue
        self.logger = logger

        // extension's delegate uses wireframe.registryDelegate which is MainActor
        // that's why init has to be MainActor as well, until properly updated
        extensionStore.allExtensions.forEach { ($0 as? ChatExtensionDelegateProvidable)?.delegate = self }
        extensionStore.delegate = self
    }
}

extension ChatExtensionsRegistry: ChatExtensionDelegate {
    func didEnableExtensions(_ extensionIds: Set<ChatExtension.Id>) {
        extensionIds.forEach { extensionId in
            guard let bot = getChatExtensionBot(for: extensionId) else {
                return
            }

            let context = ChatExtensionDiscoverContext(
                settings: settingsManager,
                storageFacade: storageFacade,
                operationQueue: operationQueue,
                logger: logger
            )

            bot.deliverAutomaticMessages(context)
        }

        changeSubject.send(.enabled(extensionIds))
    }

    func didDisableExtensions(_ extensionIds: Set<ChatExtension.Id>) {
        changeSubject.send(.disabled(extensionIds))
    }
}

extension ChatExtensionsRegistry: ChatExtensionsRegistering {
    var onChangeStream: AnyAsyncSequence<ChatExtensionRegistryChange> {
        changeSubject.eraseToAnyAsyncSequence()
    }

    func discover() {
        let context = ChatExtensionDiscoverContext(
            settings: settingsManager,
            storageFacade: storageFacade,
            operationQueue: operationQueue,
            logger: logger
        )

        extensionStore.allExtensions.forEach { chatExtension in
            guard let bot = chatExtension as? ChatExtensionBotProtocol else {
                return
            }

            bot.deliverAutomaticMessages(context)
        }

        extensionStore.startObserving()
    }

    func getExtensions(for chatId: Chat.Id) -> [ChatExtending] {
        extensionStore.allExtensions.filter { $0.activeIn(chat: chatId) }
    }

    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol? {
        extensionStore.getChatExtensionBot(for: extensionId)
    }

    func peerListMetadata(for extensionId: ChatExtension.Id) -> Chat.PeerListMetadata? {
        extensionStore.getChatExtensionBot(for: extensionId)?.peerMetadata.list
            ?? extensionStore.product(for: extensionId)?.peerListMetadata
    }

    func getWidgetProviders() -> [ChatExtensionWidgetProvider] {
        extensionStore.allExtensions.compactMap { extensionBot in
            guard let provider = extensionBot as? any ChatExtensionWidgetProvidable else {
                return nil
            }

            return ChatExtensionWidgetProvider(
                extensionId: extensionBot.identifier,
                provider: provider
            )
        }
    }
}
