import Foundation
import SDKLogger

protocol ChatMessageOrderAllocating {
    /// Returns the next counter value; `floor` is evaluated only for an empty or unreadable counter.
    func nextOrder(floor: () throws -> UInt64) throws -> UInt64
}

/// Shared allocator used by the app and NotificationServiceExtension.
/// File-based storage with flock gives cross-process uniqueness; the counter lives beside the
/// Core Data store so both share its backup exclusion.
/// Counter values start at 1; 0 marks rows that predate ordering.
final class FileChatMessageOrderAllocator: ChatMessageOrderAllocating {
    private static let counterByteCount = MemoryLayout<UInt64>.size

    private let fileURL: URL
    private let logger: LoggerProtocol

    init(fileURL: URL, logger: LoggerProtocol = Logger.shared) {
        self.fileURL = fileURL
        self.logger = logger
    }

    static let shared = FileChatMessageOrderAllocator(
        fileURL: UserStorageParams.sharedStorageDirectoryURL
            .appendingPathComponent("ChatMessageOrder.counter")
    )

    func nextOrder(floor: () throws -> UInt64) throws -> UInt64 {
        let fileDescriptor = open(fileURL.path, O_RDWR | O_CREAT, 0o600)
        guard fileDescriptor >= 0 else {
            throw ChatMessageOrderAllocatorError.systemCall(name: "open", errno: errno)
        }

        defer { close(fileDescriptor) }
        try lockExclusively(fileDescriptor)
        defer { flock(fileDescriptor, LOCK_UN) }

        var storedValue: UInt64 = 0
        let readCount = pread(fileDescriptor, &storedValue, Self.counterByteCount, 0)

        if readCount == -1 {
            throw ChatMessageOrderAllocatorError.systemCall(name: "pread", errno: errno)
        }

        let nextValue = try storedBase(readCount: readCount, storedValue: storedValue, floor: floor) + 1
        var buffer = nextValue.littleEndian

        let writeCount = pwrite(fileDescriptor, &buffer, Self.counterByteCount, 0)
        if writeCount == -1 {
            throw ChatMessageOrderAllocatorError.systemCall(name: "pwrite", errno: errno)
        }

        guard writeCount == Self.counterByteCount else {
            throw ChatMessageOrderAllocatorError.shortWrite(byteCount: writeCount)
        }

        return nextValue
    }
}

private extension FileChatMessageOrderAllocator {
    func lockExclusively(_ fileDescriptor: Int32) throws {
        while flock(fileDescriptor, LOCK_EX) != 0 {
            guard errno == EINTR else {
                throw ChatMessageOrderAllocatorError.systemCall(name: "flock", errno: errno)
            }
        }
    }

    func storedBase(readCount: Int, storedValue: UInt64, floor: () throws -> UInt64) throws -> UInt64 {
        switch readCount {
        case Self.counterByteCount:
            return UInt64(littleEndian: storedValue)
        case 0:
            return try floor()
        default:
            logger.error("Corrupted chat message order counter: \(readCount) bytes")
            return try floor()
        }
    }
}

enum ChatMessageOrderAllocatorError: Error {
    case systemCall(name: String, errno: Int32)
    case shortWrite(byteCount: Int)
}
