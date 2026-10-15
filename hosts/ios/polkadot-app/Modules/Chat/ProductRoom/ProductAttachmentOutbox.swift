import Foundation
import os
import UniformTypeIdentifiers

/// One attachment of the user that waits in the outbox until the product has it (P10.3, 07-LATER §7.2).
struct ProductOutboxEntry: Codable, Equatable, Sendable {
    /// Where the room keeps its messages.
    enum Store: String, Codable, Sendable {
        /// The host database keeps the messages. The product receives `MessagePosted`.
        case host
        /// The product keeps the messages. The product receives `NewMessage` through `user_action`.
        case product
    }

    enum Kind: Codable, Equatable, Sendable {
        case image
        case video
        case file(name: String)
    }

    enum State: Codable, Equatable, Sendable {
        /// The host stages the bytes, or sends the message.
        case sending
        /// The last attempt failed. The user can retry or delete the attachment.
        case failed(reason: String)
    }

    /// The id of the message of the user, in the host database or in the window of the room.
    let id: String
    let productId: String
    let roomId: String
    let store: Store
    let kind: Kind
    let mimeType: String
    let sizeBytes: Int
    let width: UInt32?
    let height: UInt32?
    /// The BlurHash of an image.
    let placeholder: String?
    /// Milliseconds since the Unix epoch.
    let createdAt: UInt64
    /// The preimage key, after the core staged the bytes. A retry then sends the message again and does not
    /// stage again.
    var key: Data?
    var state: State

    var chatId: Chat.Id {
        .chatExtension(productId, roomId: roomId)
    }

    var isFailed: Bool {
        if case .failed = state { true } else { false }
    }
}

/// Keeps the outbox entries and their bytes in files, so that an upload survives the close of the room screen and
/// a restart of the app. The bytes stay until the product has the message, or until the user deletes it.
protocol ProductOutboxStoring: Sendable {
    func save(_ entry: ProductOutboxEntry, bytes: Data) throws
    func update(_ entry: ProductOutboxEntry) throws
    func bytes(of id: String) -> Data?
    /// The file of the bytes, with the extension of the media type, for example to make a video frame.
    func fileURL(of id: String) -> URL?
    func entry(_ id: String) -> ProductOutboxEntry?
    func entries() -> [ProductOutboxEntry]
    func remove(_ id: String)
}

/// The outbox in a directory: `<id>.json` for each entry, and its bytes beside it.
final class ProductOutboxDirectory: ProductOutboxStoring, @unchecked Sendable {
    private let directory: URL
    private let lock = OSAllocatedUnfairLock()
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    init(directory: URL) {
        self.directory = directory
    }

    /// The outbox of the app, under the directory of the chat attachments.
    static let shared = ProductOutboxDirectory(
        directory: URL.applicationSupportDirectory.appending(
            path: "ChatAttachments/ProductOutbox",
            directoryHint: .isDirectory
        )
    )

    func save(_ entry: ProductOutboxEntry, bytes: Data) throws {
        try lock.withLock {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try bytes.write(
                to: bytesURL(entry),
                options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
            )
            try encoder.encode(entry).write(to: entryURL(entry.id), options: .atomic)
        }
    }

    func update(_ entry: ProductOutboxEntry) throws {
        try lock.withLock {
            guard FileManager.default.fileExists(atPath: entryURL(entry.id).path) else { return }
            try encoder.encode(entry).write(to: entryURL(entry.id), options: .atomic)
        }
    }

    func bytes(of id: String) -> Data? {
        fileURL(of: id).flatMap { try? Data(contentsOf: $0) }
    }

    func fileURL(of id: String) -> URL? {
        guard let entry = entry(id) else { return nil }
        let url = bytesURL(entry)
        return FileManager.default.fileExists(atPath: url.path) ? url : nil
    }

    func entry(_ id: String) -> ProductOutboxEntry? {
        lock.withLock {
            (try? Data(contentsOf: entryURL(id))).flatMap { try? decoder.decode(ProductOutboxEntry.self, from: $0) }
        }
    }

    func entries() -> [ProductOutboxEntry] {
        lock.withLock {
            let files = (
                try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
            ) ??
                []
            return files
                .filter { $0.pathExtension == "json" }
                .compactMap { (try? Data(contentsOf: $0)).flatMap { try? decoder.decode(
                    ProductOutboxEntry.self,
                    from: $0
                ) } }
                .sorted { $0.createdAt < $1.createdAt }
        }
    }

    func remove(_ id: String) {
        lock.withLock {
            let entry = (try? Data(contentsOf: entryURL(id))).flatMap { try? decoder.decode(
                ProductOutboxEntry.self,
                from: $0
            ) }
            try? FileManager.default.removeItem(at: entryURL(id))
            if let entry {
                try? FileManager.default.removeItem(at: bytesURL(entry))
            }
        }
    }

    private func entryURL(_ id: String) -> URL {
        directory.appending(path: Self.fileName(id) + ".json")
    }

    private func bytesURL(_ entry: ProductOutboxEntry) -> URL {
        let pathExtension = UTType(mimeType: entry.mimeType)?.preferredFilenameExtension ?? "bin"
        return directory.appending(path: Self.fileName(entry.id) + "." + pathExtension)
    }

    /// A message id can hold `:`, which a file name must not hold.
    private static func fileName(_ id: String) -> String {
        Data(id.utf8).map { String(format: "%02x", $0) }.joined()
    }
}
