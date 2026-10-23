import Foundation
import KeyDerivation
import Products
import Operation_iOS
import SubstrateSdk

/// The contacts of the chat product, read from the contacts of its rooms, for the screens of the app that list the
/// contacts of the native chat: the "+" panel and the blocked users (HX2). The native chat does not run beside the
/// chat product, so its own contacts are not read.
final class ProductContactSearch: LocalContactSearching, @unchecked Sendable {
    private let chatRepositoryFactory: ChatRepositoryMaking
    private let tldProvider: DotNsTldProviding

    init(
        chatRepositoryFactory: ChatRepositoryMaking = ChatRepositoryFactory(),
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) {
        self.chatRepositoryFactory = chatRepositoryFactory
        self.tldProvider = tldProvider
    }

    func searchContacts(usernamePrefix: String) -> AnyDataProviderRepository<Chat.Contact> {
        let prefix = usernamePrefix.lowercased()
        return repository { $0.isContact && $0.username.lowercased().hasPrefix(prefix) }
    }

    func contact(accountId: AccountId) -> AnyDataProviderRepository<Chat.Contact> {
        repository { $0.isContact && $0.accountId == accountId }
    }

    func allContacts() -> AnyDataProviderRepository<Chat.Contact> {
        repository(\.isContact)
    }

    func blockedContacts() -> AnyDataProviderRepository<Chat.Contact> {
        repository(\.isBlocked)
    }

    /// The contacts of the rooms of the chat product, newest conversation first, with the state of each.
    func contacts() async throws -> [ProductContact] {
        guard let tld = tldProvider.currentTld() else { return [] }

        let chats = try await chatRepositoryFactory
            .createRepository(forFilter: .roomChatsForExtension(BuiltInProduct.chat(for: tld)))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()

        return chats
            .sorted(by: ChatsComparator.lastMessageComparator)
            .compactMap(ProductContact.init(chat:))
    }
}

/// A contact of the chat product and the state of its conversation.
struct ProductContact: Sendable {
    let contact: Chat.Contact
    let state: Chat.RoomContact.State
    /// The room of the conversation with the contact.
    let chatId: Chat.Id

    var accountId: AccountId { contact.accountId }
    var username: String { contact.username }

    /// No request waits for an answer and the user did not block the contact, as a contact of the native chat
    /// without a chat request.
    var isContact: Bool {
        state == .established || state == .peerLeft || state == .blocked
    }

    var isBlocked: Bool {
        state == .blocked
    }

    init?(chat: Chat.LocalModel) {
        guard let roomContact = chat.roomContact, roomContact.state != .invite else { return nil }

        contact = Chat.Contact(
            accountId: roomContact.account,
            username: roomContact.name(fallback: chat.roomMetadata?.name ?? ""),
            publicKey: Data(),
            chatRequest: nil,
            ownKeyId: Chat.Contact.Own(signKeyId: "", encryptionKeyId: ""),
            isBlocked: roomContact.state == .blocked,
            devices: [],
            pendingDevicesFanOut: false
        )
        state = roomContact.state
        chatId = chat.chatId
    }
}

private extension ProductContactSearch {
    func repository(
        _ isIncluded: @escaping @Sendable (ProductContact) -> Bool
    ) -> AnyDataProviderRepository<Chat.Contact> {
        AnyDataProviderRepository(ProductContactRepository { [self] in
            try await contacts().filter(isIncluded).map(\.contact)
        })
    }
}

/// The contacts the chat product syncs to the host (``ProductContactsSource``), for the contact picker that the
/// TrUAPI `contacts` capability shows other products: the picker offers the same contacts the capability resolves.
final class ProductSyncedContactSearch: LocalContactSearching, @unchecked Sendable {
    private let source: ProductContactsSource

    init(source: ProductContactsSource = ProductChatContacts.shared) {
        self.source = source
    }

    func searchContacts(usernamePrefix: String) -> AnyDataProviderRepository<Chat.Contact> {
        let prefix = usernamePrefix.lowercased()
        return repository { !$0.isBlocked && $0.username.lowercased().hasPrefix(prefix) }
    }

    func contact(accountId: AccountId) -> AnyDataProviderRepository<Chat.Contact> {
        repository { !$0.isBlocked && $0.accountId == accountId }
    }

    func allContacts() -> AnyDataProviderRepository<Chat.Contact> {
        repository { !$0.isBlocked }
    }

    func blockedContacts() -> AnyDataProviderRepository<Chat.Contact> {
        repository(\.isBlocked)
    }

    private func repository(
        _ isIncluded: @escaping @Sendable (Chat.Contact) -> Bool
    ) -> AnyDataProviderRepository<Chat.Contact> {
        AnyDataProviderRepository(ProductContactRepository { [source] in
            source.currentContacts().filter(isIncluded)
        })
    }
}

/// Reads the contacts of the chat product. It writes nothing.
private final class ProductContactRepository: DataProviderRepositoryProtocol {
    typealias Model = Chat.Contact

    struct Unsupported: Error {}

    private let load: @Sendable () async throws -> [Chat.Contact]

    init(load: @escaping @Sendable () async throws -> [Chat.Contact]) {
        self.load = load
    }

    func fetchOperation(
        by modelIdClosure: @escaping () throws -> String,
        options _: RepositoryFetchOptions
    ) -> BaseOperation<Model?> {
        let load = load
        return AsyncClosureOperation { completion in
            Task {
                do {
                    let modelId = try modelIdClosure()
                    try await completion(.success(load().first { $0.identifier == modelId }))
                } catch {
                    completion(.failure(error))
                }
            }
        }
    }

    func fetchAllOperation(with _: RepositoryFetchOptions) -> BaseOperation<[Model]> {
        let load = load
        return AsyncClosureOperation { completion in
            Task {
                do {
                    try await completion(.success(load()))
                } catch {
                    completion(.failure(error))
                }
            }
        }
    }

    func fetchOperation(by _: RepositorySliceRequest, options _: RepositoryFetchOptions) -> BaseOperation<[Model]> {
        BaseOperation.createWithError(Unsupported())
    }

    func saveOperation(
        _: @escaping () throws -> [Model],
        _: @escaping () throws -> [String]
    ) -> BaseOperation<Void> {
        BaseOperation.createWithError(Unsupported())
    }

    func replaceOperation(_: @escaping () throws -> [Model]) -> BaseOperation<Void> {
        BaseOperation.createWithError(Unsupported())
    }

    func fetchCountOperation() -> BaseOperation<Int> {
        BaseOperation.createWithError(Unsupported())
    }

    func deleteAllOperation() -> BaseOperation<Void> {
        BaseOperation.createWithError(Unsupported())
    }
}
