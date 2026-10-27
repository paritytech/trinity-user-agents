import Foundation
import os

/// Keeps the messages of the outbox, so that a message survives the close of the room screen and a restart of the
/// app.
protocol ProductOutboxStoring: Sendable {
    func save(_ message: ProductOutgoingMessage) throws
    func update(_ message: ProductOutgoingMessage) throws
    func entry(_ id: String) -> ProductOutgoingMessage?
    /// Oldest first.
    func entries() -> [ProductOutgoingMessage]
    func remove(_ id: String)
}

/// The messages in a directory: `<id>.json` for each message.
final class ProductOutboxDirectory: ProductOutboxStoring, @unchecked Sendable {
    static let shared = ProductOutboxDirectory(
        directory: URL.applicationSupportDirectory.appending(
            path: "ChatAttachments/ProductOutbox",
            directoryHint: .isDirectory
        )
    )

    private let directory: URL
    private let lock = OSAllocatedUnfairLock()

    init(directory: URL) {
        self.directory = directory
    }

    func save(_ message: ProductOutgoingMessage) throws {
        try lock.withLock {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try JSONEncoder().encode(message).write(to: url(message.id), options: .atomic)
        }
    }

    func update(_ message: ProductOutgoingMessage) throws {
        try lock.withLock {
            guard FileManager.default.fileExists(atPath: url(message.id).path) else { return }
            try JSONEncoder().encode(message).write(to: url(message.id), options: .atomic)
        }
    }

    func entry(_ id: String) -> ProductOutgoingMessage? {
        lock.withLock { read(url(id)) }
    }

    func entries() -> [ProductOutgoingMessage] {
        lock.withLock {
            let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil))
                ?? []
            return files
                .filter { $0.pathExtension == "json" }
                .compactMap(read)
                .sorted { $0.createdAt < $1.createdAt }
        }
    }

    func remove(_ id: String) {
        lock.withLock {
            try? FileManager.default.removeItem(at: url(id))
        }
    }

    private func read(_ url: URL) -> ProductOutgoingMessage? {
        (try? Data(contentsOf: url)).flatMap { try? JSONDecoder().decode(ProductOutgoingMessage.self, from: $0) }
    }

    /// A file name of the hex of the id, so that any id makes a file name.
    private func url(_ id: String) -> URL {
        directory.appending(path: Data(id.utf8).map { String(format: "%02x", $0) }.joined() + ".json")
    }
}

/// The messages of an outbox in memory.
final class ProductOutboxMemory: ProductOutboxStoring, @unchecked Sendable {
    private let messages = OSAllocatedUnfairLock(initialState: [ProductOutgoingMessage]())

    func save(_ message: ProductOutgoingMessage) throws {
        messages.withLock { $0.append(message) }
    }

    func update(_ message: ProductOutgoingMessage) throws {
        messages.withLock { messages in
            guard let index = messages.firstIndex(where: { $0.id == message.id }) else { return }
            messages[index] = message
        }
    }

    func entry(_ id: String) -> ProductOutgoingMessage? {
        messages.withLock { $0.first { $0.id == id } }
    }

    func entries() -> [ProductOutgoingMessage] {
        messages.withLock { $0 }
    }

    func remove(_ id: String) {
        messages.withLock { $0.removeAll { $0.id == id } }
    }
}
