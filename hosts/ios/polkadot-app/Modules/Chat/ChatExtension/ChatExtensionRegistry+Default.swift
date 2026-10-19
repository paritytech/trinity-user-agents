import Foundation
import Keystore_iOS
import Operation_iOS

@MainActor
extension ChatExtensionsRegistry {
    static func createDefault(
        productBotProvider: ProductBotProviding
    ) -> ChatExtensionsRegistering {
        let storageFacade = UserDataStorageFacade.shared

        let reactionRepository = ChatReactionRepository(
            repository: AnyDataProviderRepository(
                storageFacade.createRepository(
                    filter: nil,
                    sortDescriptors: [],
                    mapper: AnyCoreDataMapper(ChatMessageReactionMapper())
                )
            )
        )

        let commonExtensions: [ChatExtending] = [
            ChatReactionExtension(reactionRepository: reactionRepository)
        ]

        let extensionStore = ChatExtensionStore(
            staticExtensions: commonExtensions,
            productBotProvider: productBotProvider
        )

        let registry = ChatExtensionsRegistry(
            extensionStore: extensionStore,
            storageFacade: storageFacade,
            settingsManager: SettingsManager.shared
        )

        return registry
    }
}
