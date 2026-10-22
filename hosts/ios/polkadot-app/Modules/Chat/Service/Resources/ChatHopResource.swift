import Foundation

/// A file of the chat product in the HOP pool of the native chat: the root entry `identifier` on `node`, claimed and
/// opened with `ticket`. The ticket reads the file, so the resource stays on this device.
struct ChatHopResource: Hashable, Sendable, Codable {
    let identifier: Data
    let ticket: Data
    let node: String
    /// The size the sender states.
    let size: UInt64
}

/// An opened HOP file and the pool entries that the node drops once the app keeps the bytes.
struct ChatHopFile: Sendable {
    let bytes: Data
    let entries: [Data]
}

/// Opens the HOP files of the chat product through the core, which claims them only from the app's HOP nodes.
protocol ChatHopFileOpening: Sendable {
    /// Opens the file. `progress` gets the bytes received and the size of the file while they arrive.
    func open(
        _ resource: ChatHopResource,
        progress: @escaping @Sendable (_ received: UInt64, _ total: UInt64) -> Void
    ) async throws -> ChatHopFile
    /// Tells the node that the entries of an opened file were taken. Call it once the bytes are kept.
    func ack(_ resource: ChatHopResource, entries: [Data]) async throws
}
