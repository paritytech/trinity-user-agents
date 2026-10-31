import Foundation

/// Picks the message source of a room. Every room reads the host database.
actor ChatRoomSourceContext {
    private let coreDataSource: CoreDataMessageSource

    init(coreDataSource: CoreDataMessageSource) {
        self.coreDataSource = coreDataSource
    }

    func source() async -> ChatMessageSourcing {
        coreDataSource
    }
}
