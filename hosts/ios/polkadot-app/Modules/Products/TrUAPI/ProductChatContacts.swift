import Foundation
import TrUAPIHost

/// The contacts the chat product gives (`publishContacts`), for the parts of the app outside the chat. Kept in a file
/// under Application Support that is excluded from backup and readable once the device was first unlocked.
final class ProductChatContacts: @unchecked Sendable {
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
        try lock.withLock {
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
        }
    }

    /// The contacts of every product.
    func contacts() -> [Contact] {
        lock.withLock { all() }
    }

    private func all() -> [Contact] {
        if let cached { return cached }
        let read = (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode([Contact].self, from: $0) } ?? []
        cached = read
        return read
    }
}
