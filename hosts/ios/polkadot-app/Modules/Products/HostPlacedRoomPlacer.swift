import Foundation
import Keystore_iOS
import Operation_iOS
import Products

/// Draws a host-placed product's chat room before that product has run, so a first launch is not
/// an empty list while the worker downloads and boots. Pocket bundles a card's face for the same
/// reason; Android's chat side uses `ProductChatExtension.defaultRoomMetadata`.
///
/// The product's own `createRoom` finds this row and answers `exists`, because both address it by
/// `(extensionId, roomId)`. That answer returns before metadata is written and nothing updates a
/// room afterwards, so the row keeps this name and a nil icon for good.
///
/// It does not cover a product that never resolves: `ContactsListInteractor` draws an extension
/// row only while a bot is registered, and `ProductBotFactory` builds none without a worker.
/// Fixing that means letting `ChatExtensionStore.diffAndApply` swap a bot, which it cannot.
struct HostPlacedRoomPlacer {
    private let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    private let tldProvider: DotNsTldProviding
    private let settingsManager: SettingsManagerProtocol
    private let logger: LoggerProtocol

    init(
        chatRepositoryFactory: ChatRepositoryMaking = ChatRepositoryFactory(),
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        chatRepository = chatRepositoryFactory.createRepository(forFilter: nil)
        self.tldProvider = tldProvider
        self.settingsManager = settingsManager
        self.logger = logger
    }

    /// Places every missing room. Safe to run on every launch.
    func placeRooms() async {
        guard settingsManager.isHostPlacementEnabled else { return }

        // Bounded: this runs detached, so it must not outlive the reason it was started.
        guard let tld = await tldProvider.tldRetrying(attempts: Self.tldAttempts) else { return }

        for hostPlaced in HostPlacedProducts.all {
            await place(hostPlaced, tld: tld)
        }
    }
}

private extension HostPlacedRoomPlacer {
    /// Roughly a minute, after which the rooms are left to the next launch.
    static var tldAttempts: Int { 20 }

    /// A failure costs the placeholder, not the room: the product's own `createRoom` still runs.
    func place(_ hostPlaced: HostPlacedProduct, tld: String) async {
        let extensionId = hostPlaced.productId(tld)
        let chatId = Chat.Id.chatExtension(extensionId, roomId: hostPlaced.roomId)

        do {
            let existing = try await chatRepository
                .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
                .asyncExecute()

            guard existing == nil else { return }

            let chat = Chat.LocalModel.newChatWithRoom(
                extensionId: extensionId,
                roomId: hostPlaced.roomId,
                roomMetadata: Chat.RoomMetadata(
                    chatRelativeId: hostPlaced.roomId,
                    name: hostPlaced.fallbackName,
                    icon: nil
                )
            )

            try await chatRepository.saveOperation({ [chat] }, { [] }).asyncExecute()
        } catch {
            logger.error("Failed to place the room for \(extensionId): \(error)")
        }
    }
}
