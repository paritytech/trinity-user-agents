import Foundation
import os
import Products
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// What one render stream does before it ends.
private enum ServedStream {
    case fail(Error)
    case drawThenFail(Error)
    case draw
    /// Draws and stays open, as a live render stream of the core does.
    case drawThenStayOpen
}

/// Serves the scripted streams in order, repeating the last one, and counts the
/// streams it opened so a test can tell a reopen from a first attempt.
private final class ScriptedRenderRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: 0)
    private let script: [ServedStream]

    private let terminations = OSAllocatedUnfairLock(initialState: 0)

    init(_ script: [ServedStream]) {
        self.script = script
    }

    var renderCallCount: Int { state.withLock { $0 } }
    /// The streams that ended because the view model stopped to read them.
    var terminatedStreams: Int { terminations.withLock { $0 } }

    func renderMessage(
        roomId _: String?,
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        let attempt = state.withLock { count -> Int in
            count += 1
            return count
        }
        let served = script[min(attempt, script.count) - 1]

        return AsyncThrowingStream { continuation in
            switch served {
            case let .fail(error):
                continuation.finish(throwing: error)
            case let .drawThenFail(error):
                continuation.yield(.native(.spacer(modifiers: [])))
                continuation.finish(throwing: error)
            case .draw:
                continuation.yield(.native(.spacer(modifiers: [])))
                continuation.finish()
            case .drawThenStayOpen:
                continuation.yield(.native(.spacer(modifiers: [])))
                continuation.onTermination = { [terminations] _ in
                    terminations.withLock { $0 += 1 }
                }
            }
        }
    }

    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {}
    func onUserMessage(text _: String, roomId _: String?) async throws {}
    func dispatchEvent(
        roomId _: String?,
        messageId _: String,
        messageType _: String?,
        actionId _: String,
        payload _: String?
    ) async {}

    func dispatchActionTrigger(roomId _: String?, messageId _: String, actionId _: String) async {}
    func dispatchCommand(_: Chat.Command, roomId _: String?) async -> Bool { false }
    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
    func dispose() async {}
}

@Suite("ProductWidgetViewModel", .timeLimit(.minutes(1)))
struct ProductWidgetViewModelTests {
    /// A view model whose body a view shows, so its stream runs.
    private func makeViewModel(runtime: ChatRuntimeProtocol) async -> ProductWidgetViewModel {
        let viewModel = makeHiddenViewModel(runtime: runtime)
        await MainActor.run { viewModel.bodyAppeared() }
        return viewModel
    }

    private func makeHiddenViewModel(runtime: ChatRuntimeProtocol) -> ProductWidgetViewModel {
        ProductWidgetViewModel(
            roomId: "room",
            messageId: "m1",
            messageType: "t",
            messageData: Data(),
            runtime: runtime,
            tokenResolver: StubWidgetDesignTokenResolver(),
            logger: Logger.shared,
            retryDelay: .milliseconds(10)
        )
    }

    /// The cell observes this instance, so a retry that publishes `node` reaches
    /// the screen without the decoder replacing anything.
    @Test func aFailedStreamIsRetriedOnTheSameInstance() async {
        let runtime = ScriptedRenderRuntime([.fail(ProductRuntimeError.Closed), .draw])
        let viewModel = await makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 2)
        let node = await MainActor.run { viewModel.node }
        #expect(node != nil)
    }

    /// A drop after the body arrived reopens the stream too — the product must be
    /// able to update the body again — and the tree stays on screen meanwhile.
    @Test func aStreamThatDrewThenFailedIsReopened() async {
        let runtime = ScriptedRenderRuntime([.drawThenFail(ProductRuntimeError.Closed), .draw])
        let viewModel = await makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 2)
        let node = await MainActor.run { viewModel.node }
        #expect(node != nil)
    }

    /// Without a cap a permanently broken body would reopen a core render stream
    /// for as long as the bot lives.
    @Test func retriesAreCapped() async {
        let runtime = ScriptedRenderRuntime([.fail(ProductRuntimeError.Closed)])
        let viewModel = await makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 3)
        let node = await MainActor.run { viewModel.node }
        #expect(node == nil)
    }

    /// A disposed runtime fails every stream with `CancellationError`; reopening
    /// would only log the same failure three times.
    @Test func aDisposedRuntimeIsNotRetried() async {
        let runtime = ScriptedRenderRuntime([.fail(CancellationError())])
        // Held for the whole test: a released view model cancels its own task
        // before the first stream opens, which would pass this for the wrong reason.
        let viewModel = await makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 1)
        let node = await MainActor.run { viewModel.node }
        #expect(node == nil)
    }

    @Test func aRenderThatDrawsIsNotRetried() async {
        let runtime = ScriptedRenderRuntime([.draw])
        let viewModel = await makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 1)
        let node = await MainActor.run { viewModel.node }
        #expect(node != nil)
    }

    /// The core holds the worker for each open render stream, so a body that no view shows opens none
    /// (02-DESIGN §5.2).
    @Test func noStreamOpensBeforeAViewShowsTheBody() async throws {
        let runtime = ScriptedRenderRuntime([.drawThenStayOpen])
        let viewModel = makeHiddenViewModel(runtime: runtime)

        try await Task.sleep(for: .milliseconds(50))

        #expect(runtime.renderCallCount == 0)
        #expect(viewModel.renderTask == nil)
    }

    /// A body that leaves the screen closes its stream and keeps its tree. It opens a new stream when it comes
    /// back.
    @Test func aBodyThatLeavesTheScreenClosesItsStreamAndKeepsItsTree() async throws {
        let runtime = ScriptedRenderRuntime([.drawThenStayOpen])
        let viewModel = await makeViewModel(runtime: runtime)
        try await waitUntil { await MainActor.run { viewModel.node != nil } }

        await MainActor.run { viewModel.bodyDisappeared() }
        try await waitUntil { runtime.terminatedStreams == 1 }

        #expect(await MainActor.run { viewModel.node != nil })

        await MainActor.run { viewModel.bodyAppeared() }
        try await waitUntil { runtime.renderCallCount == 2 }
    }

    /// Two cells can show one body for a moment. The stream stays open while one of them shows it.
    @Test func theStreamStaysOpenWhileOneViewShowsTheBody() async throws {
        let runtime = ScriptedRenderRuntime([.drawThenStayOpen])
        let viewModel = await makeViewModel(runtime: runtime)
        await MainActor.run { viewModel.bodyAppeared() }
        try await waitUntil { runtime.renderCallCount == 1 }

        await MainActor.run { viewModel.bodyDisappeared() }
        try await Task.sleep(for: .milliseconds(50))

        #expect(runtime.terminatedStreams == 0)
        #expect(runtime.renderCallCount == 1)
    }

    private func waitUntil(_ condition: @escaping @Sendable () async -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(2)
        while await !condition() {
            guard ContinuousClock.now < deadline else {
                Issue.record("the condition did not become true in time")
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}
