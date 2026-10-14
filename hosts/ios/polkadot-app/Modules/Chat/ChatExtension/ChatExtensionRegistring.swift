import Foundation
import AsyncExtensions
import Keystore_iOS

protocol ChatExtensionsRegistering {
    var onChangeStream: AnyAsyncSequence<ChatExtensionRegistryChange> { get }

    func discover()

    func getExtensions(for chatId: Chat.Id) -> [ChatExtending]
    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol?
    func getWidgetProviders() -> [ChatExtensionWidgetProvider]
}

extension ChatExtensionsRegistering {
    func hasChatExtension(for extensionId: ChatExtension.Id) -> Bool {
        getChatExtensionBot(for: extensionId) != nil
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

    func entryRoute(for model: ChatOpenModel) async -> ChatExtensionEntryRoute {
        guard let chatExtension = getOwningExtension(for: model) else {
            return .chat(model)
        }

        return await chatExtension.entryRoute(for: model)
    }
}

enum ChatExtensionRegistryChange {
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
