import Foundation
import Operation_iOS

final class ChatOpenService {
    let host = "chat"

    let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    private let moduleNavigator: ModuleNavigating
    private let remoteContactResolver: RemoteContactResolving

    init(
        storageFacade: StorageFacadeProtocol,
        moduleNavigator: ModuleNavigating,
        remoteContactResolver: RemoteContactResolving
    ) {
        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )
        self.moduleNavigator = moduleNavigator
        self.remoteContactResolver = remoteContactResolver
    }
}

extension ChatOpenService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard url.host() == host else {
            return false
        }

        guard
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
            let queryItems = components.queryItems,
            let rawChatId = queryItems.first(where: { $0.name == "id" })?.value,
            let chatId = Chat.Id.fromRawRepresentation(rawChatId),
            let shouldForce = queryItems.first(where: { $0.name == "force" })?.value.flatMap(Bool.init)
        else {
            return false
        }

        Task { @MainActor in
            guard !shouldForce else {
                moduleNavigator.openChat(chatId)
                return
            }

            let chat = try await chatRepository
                .fetchOperation(by: { rawChatId }, options: RepositoryFetchOptions())
                .asyncExecute()

            if let chat {
                moduleNavigator.openChat(chat.chatId)
                return
            }

            guard case let .person(accountId) = chatId else {
                return
            }

            let remoteContact = try await remoteContactResolver.fetch(by: accountId)

            guard let remoteContact else {
                return
            }

            let newRequest = try ChatOpenModel.NewRequest(
                remoteContact: remoteContact,
                ownKeyId: Chat.Contact.Own.main()
            )
            moduleNavigator.openChat(.newRequest(newRequest))
        }

        return true
    }
}
