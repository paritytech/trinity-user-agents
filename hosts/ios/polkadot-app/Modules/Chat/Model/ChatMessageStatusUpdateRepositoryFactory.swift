import Foundation
import Operation_iOS
import CoreData

protocol ChatMessageStatusUpdateRepositoryMaking {
    func createRepository(forFilter filter: NSPredicate?) -> AnyDataProviderRepository<Chat.ChatMessageStatusUpdate>
}

final class ChatMessageStatusUpdateRepositoryFactory: ChatMessageStatusUpdateRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository(forFilter filter: NSPredicate?) -> AnyDataProviderRepository<Chat.ChatMessageStatusUpdate> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageStatusUpdateMapper())
            )
        )
    }
}
