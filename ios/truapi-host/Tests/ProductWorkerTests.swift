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
        var executions = runtime.workerExecutionState(productId: product).compactMap { state -> NativeWorkerExecution? in
            guard case let .running(execution) = state else { return nil }
            return execution
        }.makeAsyncIterator()

        let chat = runtime.claimWorker(productId: product)
        let pocket = runtime.claimWorker(productId: product)
        let attempt = try #require(await starts.next())
        let owner = try attempt.openExecution(bridge: StubHostBridge())
        let socket = try connect(owner)
        defer { socket.cancel(with: .normalClosure, reason: nil) }
        #expect(attempt.moduleStarted())
        let execution = try #require(await executions.next())
        let render = try execution.render(ProductRendererRenderRequest(context: .pocketCard(cardId: "live"), payload: Data()))
        defer { withExtendedLifetime(render) {} }
        chat.release()
        chat.release()
        try execution.publishRendererAction(item: HostRendererActionSubscribeItem(
            context: .pocketCard(cardId: "live"), actionId: "tap", payload: Data()
        ))
        pocket.release()
        #expect(await stops.next() == attempt.id())
        var states = runtime.workerExecutionState(productId: product).makeAsyncIterator()
        if case .running = await states.next() { Issue.record("a worker nobody claims is still published") }
        #expect(!attempt.moduleStarted())
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
            workerEngine: host
        )
    }
}

private final class RecordingWebViews: NativeWorkerEngine, Sendable {
    let starts = AsyncStream<NativeWorkerAttempt>.makeStream()
    let stops = AsyncStream<UInt64>.makeStream()

    func start(productId _: ProductId, attempt: NativeWorkerAttempt) {
        starts.continuation.yield(attempt)
    }

    func stop(productId _: ProductId, attemptId: UInt64) {
        stops.continuation.yield(attemptId)
    }
}
