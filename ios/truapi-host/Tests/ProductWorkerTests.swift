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
        #expect(owner.reportWorkerModuleStarted())
        let execution = try #require(await executions.next())
        let render = try execution.render(ProductRendererRenderRequest(context: .pocketCard(cardId: "live"), payload: Data()))
        defer { withExtendedLifetime(render) {} }
        chat.release()
        chat.release()
        try execution.publishRendererAction(HostRendererActionSubscribeItem(
            context: .pocketCard(cardId: "live"), actionId: "tap", payload: Data()
        ))
        pocket.release()
        #expect(await stops.next() == attempt.id())
        #expect(runtime.currentWorkerExecution(productId: product) == nil)
        #expect(!owner.reportWorkerModuleStarted())
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

private final class RecordingWebViews: NativeHostWebViewWorker, Sendable {
    let starts = AsyncStream<NativeWorkerAttempt>.makeStream()
    let stops = AsyncStream<UInt64>.makeStream()

    func start(productId _: ProductId, attempt: NativeWorkerAttempt) {
        starts.continuation.yield(attempt)
    }

    func stop(productId _: ProductId, attemptId: UInt64) {
        stops.continuation.yield(attemptId)
    }
}
