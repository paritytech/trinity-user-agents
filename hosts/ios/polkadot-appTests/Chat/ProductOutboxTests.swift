import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The outbox of the messages of the user in product rooms keeps each message until the product has it, as the
/// outbox of the native chat does, and sends it again under the id that the host chose.
struct ProductOutboxTests {
    /// A message that waits from before the app started goes when the app starts, under its id.
    @Test func aWaitingMessageGoesWhenTheAppStarts() async throws {
        let directory = ProductOutboxDirectory(directory: temporaryDirectory())
        let delivery = ScriptedDelivery()
        let message = ProductOutgoingMessage(
            id: UUID().uuidString,
            productId: "chat.dot",
            roomId: "dm-7",
            content: .text("hi", replyTo: "m1"),
            createdAt: 1,
            state: .sending
        )
        try directory.save(message)

        let outbox = ProductOutbox(directory: directory, workers: StubWorkers(), delivery: delivery, settings: fast)
        await outbox.resume()
        await waitUntil { directory.entries().isEmpty }

        #expect(delivery.delivered.map(\.id) == [message.id])
        #expect(delivery.delivered.first?.replyTo == "m1")
    }

    /// The messages of a room go in the order of the user. A refused message stays unsent, and the next one goes.
    @Test func aRefusedMessageStaysAndTheNextOneGoes() async throws {
        let directory = ProductOutboxMemory()
        let delivery = ScriptedDelivery { message in
            if case .text("too long", _) = message.content {
                throw ProductOutboxRefusal(reason: "Too long")
            }
        }
        let outbox = ProductOutbox(directory: directory, workers: StubWorkers(), delivery: delivery, settings: fast)

        let refused = try await outbox.send(text: "too long", replyTo: nil, productId: "chat.dot", roomId: "dm-7")
        let next = try await outbox.send(text: "short", replyTo: nil, productId: "chat.dot", roomId: "dm-7")
        await waitUntil { directory.entries().count == 1 }

        #expect(delivery.attempts.map(\.id) == [refused.id, next.id])
        #expect(directory.entries().map(\.id) == [refused.id])
        #expect(directory.entries().first?.state == .refused)
        #expect(await !outbox.waitUntilDelivered(refused.id))
        #expect(await outbox.waitUntilDelivered(next.id))
    }

    /// A message that did not go goes again at once when the worker of the product starts again.
    @Test func aWorkerStartSendsAWaitingMessage() async throws {
        let directory = ProductOutboxMemory()
        let workers = StubWorkers()
        let failing = OSAllocatedUnfairLock(initialState: true)
        let delivery = ScriptedDelivery { _ in
            if failing.withLock({ $0 }) { throw ChatQueryError.noAnswer }
        }
        let slow = ProductOutbox.Settings(firstRetryDelay: .seconds(60), maxRetryDelay: .seconds(60))
        let outbox = ProductOutbox(directory: directory, workers: workers, delivery: delivery, settings: slow)

        let message = try await outbox.send(text: "hi", replyTo: nil, productId: "chat.dot", roomId: "dm-7")
        await waitUntil { delivery.attempts.count == 1 }
        try await Task.sleep(for: .milliseconds(20))

        failing.withLock { $0 = false }
        workers.startWorker()
        await waitUntil { directory.entries().isEmpty }

        #expect(delivery.attempts.map(\.id) == [message.id, message.id])
        #expect(delivery.delivered.map(\.id) == [message.id])
    }

    private let fast = ProductOutbox.Settings(firstRetryDelay: .milliseconds(5), maxRetryDelay: .milliseconds(20))
}

private final class ScriptedDelivery: ProductOutboxDelivering, @unchecked Sendable {
    private let answer: @Sendable (ProductOutgoingMessage) async throws -> Void
    private let lock = NSLock()
    private var tried: [ProductOutgoingMessage] = []
    private var done: [ProductOutgoingMessage] = []

    init(answer: @escaping @Sendable (ProductOutgoingMessage) async throws -> Void = { _ in }) {
        self.answer = answer
    }

    var attempts: [ProductOutgoingMessage] {
        lock.withLock { tried }
    }

    var delivered: [ProductOutgoingMessage] {
        lock.withLock { done }
    }

    func deliver(_ message: ProductOutgoingMessage, body _: ChatMessageBody) async throws -> ChatMessage? {
        lock.withLock { tried.append(message) }
        try await answer(message)
        lock.withLock { done.append(message) }
        return nil
    }
}

private final class StubWorkers: ProductOutboxWorkers, @unchecked Sendable {
    private let starts = OSAllocatedUnfairLock<[AsyncStream<Void>.Continuation]>(initialState: [])

    func holdWorker(productId _: String) async {}

    func releaseWorker(productId _: String) {}

    func workerStarts(productId _: String) -> AsyncStream<Void> {
        let (stream, continuation) = AsyncStream<Void>.makeStream()
        starts.withLock { $0.append(continuation) }
        return stream
    }

    func startWorker() {
        starts.withLock { $0 }.forEach { $0.yield() }
    }
}

private func temporaryDirectory() -> URL {
    FileManager.default.temporaryDirectory.appending(path: "ProductOutboxTests-\(UUID().uuidString)")
}

private func waitUntil(_ condition: @Sendable () async -> Bool) async {
    for _ in 0 ..< 1_000 where await !condition() {
        try? await Task.sleep(for: .milliseconds(2))
    }
}
