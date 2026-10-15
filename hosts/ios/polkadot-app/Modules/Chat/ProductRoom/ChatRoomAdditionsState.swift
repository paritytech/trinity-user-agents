import Foundation
import UIKit

/// The services that a product room screen uses for the Chat Extension v2 additions (07-LATER §5, §9).
struct ChatRoomAdditionsDependencies {
    let metaStore: ChatMessageMetaStoring?
    let personaStore: ChatProductPersonaStoring?
    let preparingMessages: ChatPreparingMessageTracking?
    let eventPublisher: ChatRoomEventPublishing?
    let readAloud: ChatReadAloudServicing?
    let threadRoots: ChatThreadRootResolving?
    /// How long a branch control waits for the answer of the product before it goes back (07-LATER §9.3).
    var branchTimeout: Duration = .seconds(5)

    /// No service: the room draws no meta, no persona and no thread, and sends no room event.
    static let inert = ChatRoomAdditionsDependencies(
        metaStore: nil,
        personaStore: nil,
        preparingMessages: nil,
        eventPublisher: nil,
        readAloud: nil,
        threadRoots: nil
    )

    /// The services of the app.
    @MainActor
    static func live(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) -> Self {
        ChatRoomAdditionsDependencies(
            metaStore: ChatMessageMetaStore(databaseService: storageFacade.databaseService),
            personaStore: ChatProductPersonaStore(storageFacade: storageFacade),
            preparingMessages: ChatPreparingMessageStore.shared,
            eventPublisher: RootDependencyLocator.getDependency(),
            readAloud: ChatReadAloudService.shared,
            threadRoots: ChatThreadRootResolver.shared
        )
    }
}

/// What a product room screen knows of the additions of its room: the personas of the product, the meta of the
/// messages, the reasoning rows that the user expanded, the branch arrows that wait for the product, and the root
/// of a thread room.
@MainActor
final class ChatRoomAdditionsState {
    /// An arrow of a branch control that waits for the answer of the product.
    struct PendingBranch {
        let index: UInt16
        let timeout: Task<Void, Never>
    }

    let dependencies: ChatRoomAdditionsDependencies

    /// The personas of the product, by bot id.
    var personas: [String: Chat.ProductPersona] = [:]
    /// The loaded icons of the personas, by bot id.
    var personaIcons: [String: UIImage] = [:]
    var meta: Chat.MessageMetaIndex = .empty
    /// The ids under which the product knows the messages that are `Preparing`.
    var preparing: Set<String> = []
    /// The ids under which the product knows the reasoning rows that the user expanded.
    var expandedReasoning: Set<String> = []
    /// The arrows that wait for the product, by the id under which the product knows the message.
    var pendingBranches: [String: PendingBranch] = [:]
    /// The parent and the root of a thread room. Nil for a room that is not a thread, or until it loads.
    var threadRoot: ChatThreadRoot?
    /// The parent of the thread whose root `threadRoot` shows.
    var threadParent: Chat.ThreadParent?
    /// The id under which the product knows the message that the screen shows when it appears again, after the
    /// user comes back from its thread.
    var returnToMessageId: Chat.MessageId?
    /// The room screen shows now.
    var isOnScreen = false
    /// The activity line that the product set, so the line can show a persona that loads later.
    var activity: Chat.RoomActivity?

    var tasks: [Task<Void, Never>] = []

    init(dependencies: ChatRoomAdditionsDependencies) {
        self.dependencies = dependencies
    }

    deinit {
        tasks.forEach { $0.cancel() }
        pendingBranches.values.forEach { $0.timeout.cancel() }
    }
}
