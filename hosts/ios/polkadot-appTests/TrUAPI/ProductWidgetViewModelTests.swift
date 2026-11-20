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
}

/// Serves the scripted streams in order, repeating the last one, and counts the
/// streams it opened so a test can tell a reopen from a first attempt.
private final class ScriptedRenderRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: 0)
    private let script: [ServedStream]

    init(_ script: [ServedStream]) {
        self.script = script
    }

    var renderCallCount: Int { state.withLock { $0 } }

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
        payload _: Data?
    ) async {}
    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
    func dispose() async {}
}

@Suite("ProductWidgetViewModel", .timeLimit(.minutes(1)))
struct ProductWidgetViewModelTests {
    private func makeViewModel(runtime: ChatRuntimeProtocol) -> ProductWidgetViewModel {
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
        let viewModel = makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 2)
        let node = await MainActor.run { viewModel.node }
        #expect(node != nil)
    }

    /// A drop after the body arrived reopens the stream too — the product must be
    /// able to update the body again — and the tree stays on screen meanwhile.
    @Test func aStreamThatDrewThenFailedIsReopened() async {
        let runtime = ScriptedRenderRuntime([.drawThenFail(ProductRuntimeError.Closed), .draw])
        let viewModel = makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 2)
        let node = await MainActor.run { viewModel.node }
        #expect(node != nil)
    }

    /// Every resume that closes the product connection fails each open render
    /// after it drew. A cell that stopped reopening after a few of them would
    /// show a body the product no longer renders, and the product ignores taps
    /// on a body it is not rendering.
    @Test func aBodyIsReopenedAfterEveryDropThatFollowsADraw() async {
        let dropped = ServedStream.drawThenFail(ProductRuntimeError.Closed)
        let runtime = ScriptedRenderRuntime([dropped, dropped, dropped, dropped, .draw])
        let viewModel = makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 5)
    }

    /// Without a cap a permanently broken body would reopen a core render stream
    /// for as long as the bot lives.
    @Test func retriesAreCapped() async {
        let runtime = ScriptedRenderRuntime([.fail(ProductRuntimeError.Closed)])
        let viewModel = makeViewModel(runtime: runtime)

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
        let viewModel = makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 1)
        let node = await MainActor.run { viewModel.node }
        #expect(node == nil)
    }

    @Test func aRenderThatDrawsIsNotRetried() async {
        let runtime = ScriptedRenderRuntime([.draw])
        let viewModel = makeViewModel(runtime: runtime)

        await viewModel.renderTask?.value

        #expect(runtime.renderCallCount == 1)
        let node = await MainActor.run { viewModel.node }
        #expect(node != nil)
    }

    private func makeViewModel(serving outputs: [ChatRendererOutput]) -> ProductWidgetViewModel {
        ProductWidgetViewModel(
            messageId: "m1",
            render: {
                AsyncThrowingStream { continuation in
                    outputs.forEach { continuation.yield($0) }
                    continuation.finish()
                }
            },
            tokenResolver: StubWidgetDesignTokenResolver(),
            logger: Logger.shared,
            retryDelay: .milliseconds(10)
        )
    }

    /// An offset update moves its keyed gesture without replacing the tree.
    @Test func anOffsetUpdateMovesItsGestureWithoutANewTree() async {
        let viewModel = makeViewModel(serving: [
            .native(.spacer(modifiers: [])),
            .native(.setOffset(key: "thumb", x: 42, y: -1)),
        ])

        await viewModel.renderTask?.value

        let (offset, node) = await MainActor.run {
            (viewModel.gestureOffsets?.channel(for: "thumb").offset, viewModel.node)
        }
        #expect(offset == CGPoint(x: 42, y: -1))
        guard case .spacer = node?.content else {
            Issue.record("the tree was replaced by the offset update")
            return
        }
    }

    /// A tree that arrives after an offset update places the gesture itself.
    @Test func aTreeAfterAnOffsetUpdateWins() async {
        let viewModel = makeViewModel(serving: [
            .native(.spacer(modifiers: [])),
            .native(.setOffset(key: "thumb", x: 42, y: 0)),
            .native(.spacer(modifiers: [])),
        ])

        await viewModel.renderTask?.value

        let offset = await MainActor.run { viewModel.gestureOffsets?.channel(for: "thumb").offset }
        #expect(offset == nil)
    }
}
