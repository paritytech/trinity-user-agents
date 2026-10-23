import Foundation
import TrUAPIHost

/// The contacts of the chat product, as the parts of the app outside the chat read them: the TrUAPI `contacts`
/// capability of other products and its contact picker.
protocol ProductContactsSource: AnyObject, Sendable {
    /// The contacts now.
    func currentContacts() -> [Chat.Contact]

    /// Calls `update` with the contacts after each change, on the thread of the change, for as long as the returned
    /// token lives.
    func observe(_ update: @escaping @Sendable ([Chat.Contact]) -> Void) -> AnyObject
}

/// The contacts the chat product gives (`publishContacts`), for the parts of the app outside the chat. Kept in a file
/// under Application Support that is excluded from backup and readable once the device was first unlocked.
final class ProductChatContacts: @unchecked Sendable {
    /// The one copy the app reads and the person chat bridge writes, so a reader sees each change.
    static let shared = ProductChatContacts()

    struct Contact: Codable, Equatable {
        let productId: String
        let accountId: Data
        let username: String?
        let roomId: String
        let blocked: Bool
    }

    private let file: URL
    private let lock = NSLock()
    private var cached: [Contact]?
    private var observers: [UUID: @Sendable ([Chat.Contact]) -> Void] = [:]

    init(file: URL = ProductChatContacts.defaultFile) {
        self.file = file
    }

    static var defaultFile: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ProductChatContacts", isDirectory: true)
            .appendingPathComponent("contacts.json")
    }

    /// Replaces the contacts of `productId` and keeps those of other products.
    func replace(productId: String, contacts: [HostChatContact]) throws {
        let notified = try lock.withLock {
            let kept = all().filter { $0.productId != productId } + contacts.map {
                Contact(
                    productId: productId,
                    accountId: $0.accountId,
                    username: $0.username,
                    roomId: $0.roomId,
                    blocked: $0.blocked
                )
            }
            var directory = file.deletingLastPathComponent()
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
            try JSONEncoder().encode(kept).write(
                to: file,
                options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
            )
            cached = kept
            return (Self.chatContacts(kept), Array(observers.values))
        }
        notified.1.forEach { $0(notified.0) }
    }

    /// The contacts of every product.
    func contacts() -> [Contact] {
        lock.withLock { all() }
    }

    private func removeObserver(_ id: UUID) {
        lock.withLock { _ = observers.removeValue(forKey: id) }
    }

    private func all() -> [Contact] {
        if let cached { return cached }
        let read = (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode([Contact].self, from: $0) } ?? []
        cached = read
        return read
    }
}

extension ProductChatContacts: ProductContactsSource {
    func currentContacts() -> [Chat.Contact] {
        Self.chatContacts(contacts())
    }

    func observe(_ update: @escaping @Sendable ([Chat.Contact]) -> Void) -> AnyObject {
        let id = UUID()
        lock.withLock { observers[id] = update }
        return Observation { [weak self] in self?.removeObserver(id) }
    }
}

private extension ProductChatContacts {
    /// Ends an observation when it is released.
    final class Observation {
        private let cancel: () -> Void

        init(cancel: @escaping () -> Void) {
            self.cancel = cancel
        }

        deinit {
            cancel()
        }
    }

    /// One row per account, as the contact screens of the app show them.
    static func chatContacts(_ contacts: [Contact]) -> [Chat.Contact] {
        var seen = Set<Data>()
        return contacts.compactMap { contact in
            guard seen.insert(contact.accountId).inserted else { return nil }
            return Chat.Contact(
                accountId: contact.accountId,
                username: contact.username ?? "",
                publicKey: Data(),
                chatRequest: nil,
                ownKeyId: Chat.Contact.Own(signKeyId: "", encryptionKeyId: ""),
                isBlocked: contact.blocked,
                devices: [],
                pendingDevicesFanOut: false
            )
        }
    }
}
