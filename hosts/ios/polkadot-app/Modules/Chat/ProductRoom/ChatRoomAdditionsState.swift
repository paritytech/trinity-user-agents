import Foundation
import UIKit

/// The services that a product room screen uses for the Chat Extension v2 additions (07-LATER §5, §9).
struct ChatRoomAdditionsDependencies {
    let personaStore: ChatProductPersonaStoring?
    let eventPublisher: ChatRoomEventPublishing?
    let readAloud: ChatReadAloudServicing?

    /// No service: the room draws no persona, and sends no room event.
    static let inert = ChatRoomAdditionsDependencies(
        personaStore: nil,
        eventPublisher: nil,
        readAloud: nil
    )

    /// The services of the app.
    @MainActor
    static func live(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) -> Self {
        ChatRoomAdditionsDependencies(
            personaStore: ChatProductPersonaStore(storageFacade: storageFacade),
            eventPublisher: RootDependencyLocator.getDependency(),
            readAloud: ChatReadAloudService.shared
        )
    }
}

/// What a product room screen knows of the additions of its room: the personas of the product and the long texts
/// that the user expanded.
@MainActor
final class ChatRoomAdditionsState {
    let dependencies: ChatRoomAdditionsDependencies

    /// The personas of the product, by bot id.
    var personas: [String: Chat.ProductPersona] = [:]
    /// The loaded icons of the personas, by bot id.
    var personaIcons: [String: UIImage] = [:]
    /// The ids under which the product knows the long texts that the user expanded with "Show more".
    var expandedTexts: Set<String> = []

    var tasks: [Task<Void, Never>] = []

    init(dependencies: ChatRoomAdditionsDependencies) {
        self.dependencies = dependencies
    }

    deinit {
        tasks.forEach { $0.cancel() }
    }
}
