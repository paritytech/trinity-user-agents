import Foundation
import Keystore_iOS

@MainActor
extension ChatExtensionsRegistry {
    static func createDefault(
        productBotProvider: ProductBotProviding
    ) -> ChatExtensionsRegistering {
        let storageFacade = UserDataStorageFacade.shared

        // The reactions of the chat product's rooms come with its messages; the host adds no extension of its own.
        let commonExtensions: [ChatExtending] = []

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
