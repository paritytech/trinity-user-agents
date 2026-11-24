import Foundation
import Testing
@testable import TrUAPIHost

struct ProductWorkerTests {
    @Test(.timeLimit(.minutes(1)))
    func chatReleasesOnlyItsOwnClaimWhilePocketKeepsUsingTheWorker() async throws {
        let host = RecordingWebViews()
        let runtime = try makeRuntime(host)
        let product = try ProductId("worker.dot")
        var starts = host.starts.stream.makeAsyncIterator()
        var stops = host.stops.stream.makeAsyncIterator()
        var states = runtime.workerExecutionState(productId: product).makeAsyncIterator()

        let chat = runtime.claimWorker(productId: product)
        let pocket = runtime.claimWorker(productId: product)
        let attempt = try #require(await starts.next())
        let owner = try attempt.openExecution(bridge: StubHostBridge())
        let socket = try connect(owner)
        defer { socket.cancel(with: .normalClosure, reason: nil) }
        #expect(owner.reportReady(for: attempt))
        while let state = await states.next() {
            if case .running = state { break }
        }
        chat.release()
        chat.release()
        let execution = try #require(runtime.currentWorkerExecution(productId: product))
        try execution.publishRendererAction(HostRendererActionSubscribeItem(
            context: .pocketCard(cardId: "live"), actionId: "tap", payload: Data()
        ))
        pocket.release()
        #expect(await stops.next() == attempt.id())
        #expect(runtime.currentWorkerExecution(productId: product) == nil)
        runtime.shutdownProductWorkers()
    }

    @Test(.timeLimit(.minutes(1)))
    func aLateWebViewCannotPublishOverTheNextStartup() async throws {
        let host = RecordingWebViews()
        let runtime = try makeRuntime(host)
        let product = try ProductId("worker.dot")
        var starts = host.starts.stream.makeAsyncIterator()
        var states = runtime.workerExecutionState(productId: product).makeAsyncIterator()
        let first = runtime.claimWorker(productId: product)
        let stale = try #require(await starts.next())
        let staleOwner = try stale.openExecution(bridge: StubHostBridge())
        first.release()
        let second = runtime.claimWorker(productId: product)
        let current = try #require(await starts.next())
        let currentOwner = try current.openExecution(bridge: StubHostBridge())
        let socket = try connect(currentOwner)
        defer { socket.cancel(with: .normalClosure, reason: nil) }
        #expect(currentOwner.reportReady(for: current))
        while let state = await states.next() {
            if case .running = state { break }
        }
        #expect(!staleOwner.reportReady(for: stale))
        stale.failed(reason: "old web process terminated")
        #expect(runtime.currentWorkerExecution(productId: product) != nil)
        second.release()
        runtime.shutdownProductWorkers()
    }

    private func connect(_ owner: TrUAPIProductExecution) throws -> URLSessionWebSocketTask {
        let endpoint = try owner.startWsBridge()
        let url = try #require(URL(string: "ws://127.0.0.1:\(endpoint.port)/?t=\(endpoint.token)"))
        let socket = URLSession.shared.webSocketTask(with: url)
        socket.resume()
        return socket
    }

    private func makeRuntime(_ host: RecordingWebViews) throws -> TrUAPIHostRuntime {
        try TrUAPIHostRuntime(
            bridge: StubHostBridge(),
            runtimeConfig: HostRuntimeConfig(
                hostName: "worker-test", stack: previewnetStack(), databaseDirectory: temporaryDatabaseDirectory()
            ),
            workerHost: host
        )
    }
}

private final class RecordingWebViews: NativeWebViewWorkerHost, Sendable {
    let starts = AsyncStream<NativeWorkerAttempt>.makeStream()
    let stops = AsyncStream<UInt64>.makeStream()

    func start(productId _: ProductId, attempt: NativeWorkerAttempt) {
        starts.continuation.yield(attempt)
    }

    func stop(productId _: ProductId, attemptId: UInt64) {
        stops.continuation.yield(attemptId)
    }
}
