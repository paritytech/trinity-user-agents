import Foundation
import SubstrateSdk

enum DeviceSyncEntityChunkingError: Error {
    case emptyResult
}

/// Splits a collected snapshot of `Chat.DeviceSyncEntity`s into sub-batches whose SCALE-encoded
/// size each fits below the SCTP `max-message-size` of the WebRTC data channel.
///
/// Sending a single user message larger than the negotiated SCTP `max-message-size` aborts the
/// association (RFC 8831 §6.6), which manifests as the data channel silently closing. The chunker
/// keeps individual updates safely small without touching the wire format.
struct DeviceSyncEntityChunker {
    /// Per-update entity-list budget in bytes. The framing overhead (`DeviceSyncMessage` tag,
    /// `DeviceSyncUpdate` id/timePoint, `DataChannelMessage` envelope) is small compared to this
    /// budget, so 60 KB stays safely below the 64 KB SCTP default.
    static let defaultMaxPayloadSize: Int = 60 * 1_024

    let maxPayloadSize: Int

    init(maxPayloadSize: Int = DeviceSyncEntityChunker.defaultMaxPayloadSize) {
        self.maxPayloadSize = maxPayloadSize
    }

    func chunk(_ entities: [Chat.DeviceSyncEntity]) throws -> [[Chat.DeviceSyncEntity]] {
        var atomic: [(entity: Chat.DeviceSyncEntity, size: Int)] = []

        for entity in entities {
            for piece in try splitOversized(entity) {
                let size = try piece.scaleEncoded().count
                atomic.append((piece, size))
            }
        }

        return greedyPack(atomic)
    }
}

private extension DeviceSyncEntityChunker {
    func greedyPack(
        _ atomic: [(entity: Chat.DeviceSyncEntity, size: Int)]
    ) -> [[Chat.DeviceSyncEntity]] {
        var batches: [[Chat.DeviceSyncEntity]] = []
        var current: [Chat.DeviceSyncEntity] = []
        var currentSize = 0

        for item in atomic {
            if !current.isEmpty, currentSize + item.size > maxPayloadSize {
                batches.append(current)
                current = []
                currentSize = 0
            }
            current.append(item.entity)
            currentSize += item.size
        }

        if !current.isEmpty {
            batches.append(current)
        }

        return batches
    }

    func splitOversized(_ entity: Chat.DeviceSyncEntity) throws -> [Chat.DeviceSyncEntity] {
        let encodedSize = try entity.scaleEncoded().count

        guard encodedSize > maxPayloadSize else {
            return [entity]
        }

        switch entity {
        case let .devices(items):
            return try splitArray(items).map { .devices($0) }
        case let .chatsAdded(items):
            return try splitArray(items).map { .chatsAdded($0) }
        case let .chatsRemoved(items):
            return try splitArray(items).map { .chatsRemoved($0) }
        case let .messages(items):
            return try splitArray(items).map { .messages($0) }
        }
    }

    func splitArray<T: ScaleCodable>(_ items: [T]) throws -> [[T]] {
        var result: [[T]] = []
        var current: [T] = []
        var currentSize = 0

        for item in items {
            let size = try item.scaleEncoded().count

            if !current.isEmpty, currentSize + size > maxPayloadSize {
                result.append(current)
                current = []
                currentSize = 0
            }

            current.append(item)
            currentSize += size
        }

        if !current.isEmpty {
            result.append(current)
        }

        return result
    }
}
