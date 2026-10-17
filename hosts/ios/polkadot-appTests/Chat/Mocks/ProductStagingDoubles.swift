import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

struct StagingCall: Equatable {
    let productId: String
    let roomId: String
    let mimeType: String
    let size: Int
}

/// Opens once. A staging that waits on it stays in the `sending` state until the test opens it.
actor AsyncGate {
    private var isOpen = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        guard !isOpen else { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func open() {
        isOpen = true
        waiters.forEach { $0.resume() }
        waiters = []
    }
}

final class RecordingStaging: ProductAttachmentStaging, @unchecked Sendable {
    private let lock = OSAllocatedUnfairLock()
    private var recorded: [StagingCall] = []
    private var recordedHolds: [String] = []
    /// Each stage takes the first failure, and succeeds when none is left.
    var failures: [ProductStageFailure] = []
    var gate: AsyncGate?
    let key: Foundation.Data

    init(key: Foundation.Data) {
        self.key = key
    }

    var calls: [StagingCall] {
        lock.withLock { recorded }
    }

    var holds: [String] {
        lock.withLock { recordedHolds }
    }

    func stage(productId: String, roomId: String, mimeType: String, bytes: Foundation.Data) async throws -> Foundation
        .Data {
        let failure: ProductStageFailure? = lock.withLock {
            recorded.append(StagingCall(productId: productId, roomId: roomId, mimeType: mimeType, size: bytes.count))
            return failures.isEmpty ? nil : failures.removeFirst()
        }
        await gate?.wait()
        if let failure { throw failure }
        return key
    }

    /// Seals as the core does: the parts and the key are fixed, so that a test can name them.
    func seal(productId: String, roomId: String, bytes: Foundation.Data) async throws -> ChatSealedResource {
        let failure: ProductStageFailure? = lock.withLock {
            recorded.append(StagingCall(productId: productId, roomId: roomId, mimeType: "sealed", size: bytes.count))
            return failures.isEmpty ? nil : failures.removeFirst()
        }
        if let failure { throw failure }
        return Self.sealed(size: bytes.count)
    }

    static func sealed(size: Int) -> ChatSealedResource {
        ChatSealedResource(
            parts: [Foundation.Data(repeating: 1, count: 32), Foundation.Data(repeating: 2, count: 32)],
            key: Foundation.Data(repeating: 3, count: 32),
            nonce: Foundation.Data(repeating: 4, count: 24),
            size: UInt64(size + 16),
            digest: Foundation.Data(repeating: 5, count: 64)
        )
    }

    func holdWorker(productId: String) {
        lock.withLock { recordedHolds.append("hold \(productId)") }
    }

    func releaseWorker(productId: String) {
        lock.withLock { recordedHolds.append("release \(productId)") }
    }
}

final class RecordingDelivery: ProductOutboxDelivering, @unchecked Sendable {
    private let lock = OSAllocatedUnfairLock()
    private var recordedShown: [ProductOutboxEntry] = []
    private var recordedDelivered: [ChatMessageBody] = []
    private var recordedRemoved: [String] = []
    var answer: Result<ChatMessage?, ProductStageFailure> = .success(nil)

    var shown: [ProductOutboxEntry] { lock.withLock { recordedShown } }
    var delivered: [ChatMessageBody] { lock.withLock { recordedDelivered } }
    var removed: [String] { lock.withLock { recordedRemoved } }

    func show(_ entry: ProductOutboxEntry, preview _: ChatMessageBody) async {
        lock.withLock { recordedShown.append(entry) }
    }

    func deliver(_: ProductOutboxEntry, body: ChatMessageBody) async throws -> ChatMessage? {
        lock.withLock { recordedDelivered.append(body) }
        return try answer.get()
    }

    func remove(_ entry: ProductOutboxEntry) async {
        lock.withLock { recordedRemoved.append(entry.id) }
    }
}

struct StagerHarness {
    let outbox: ProductOutboxDirectory
    let staging: RecordingStaging
    let delivery = RecordingDelivery()
    let stager: ProductAttachmentStager

    init() throws {
        let directory = FileManager.default.temporaryDirectory.appending(path: "outbox-\(UUID().uuidString)")
        outbox = ProductOutboxDirectory(directory: directory)
        staging = RecordingStaging(key: Foundation.Data(repeating: 7, count: 32))
        stager = ProductAttachmentStager(
            outbox: outbox,
            staging: staging,
            delivery: delivery,
            media: ProductStagedMediaRegistry(),
            logger: StubLogger()
        )
    }

    static func until(_ condition: () async -> Bool) async throws {
        for _ in 0 ..< 400 where await !condition() {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(await condition())
    }
}
