import Foundation
import os
import Products
import UIKitExt

final class ReloadableChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    typealias RuntimeMaking = @Sendable () async throws -> ChatRuntimeProtocol

    private struct State {
        var runtime: ChatRuntimeProtocol
        var messagingSupport: ProductsNativeApi.MessagingSupport?
        weak var presentationView: ControllerBackedProtocol?
        var disposed = false
    }

    private let makeRuntime: RuntimeMaking
    private let state: OSAllocatedUnfairLock<State>

    init(runtime: ChatRuntimeProtocol, makeRuntime: @escaping RuntimeMaking) {
        self.makeRuntime = makeRuntime
        state = OSAllocatedUnfairLock(uncheckedState: State(runtime: runtime))
    }

    func reload() async throws {
        let replacement = try await makeRuntime()

        let swap = state.withLock { state -> (ChatRuntimeProtocol, State)? in
            guard !state.disposed else { return nil }

            let previous = state.runtime
            state.runtime = replacement
            return (previous, state)
        }

        guard let (previous, snapshot) = swap else {
            await replacement.dispose()
            return
        }

        await previous.dispose()

        if let messagingSupport = snapshot.messagingSupport {
            try await replacement.start(messagingSupport: messagingSupport)
        }

        if let presentationView = snapshot.presentationView {
            await MainActor.run { replacement.attach(presentationView: presentationView) }
        }
    }

    func start(messagingSupport: ProductsNativeApi.MessagingSupport) async throws {
        let runtime = state.withLock { state in
            state.messagingSupport = messagingSupport
            return state.runtime
        }

        try await runtime.start(messagingSupport: messagingSupport)
    }

    func onUserMessage(text: String, roomId: String?) async throws {
        try await currentRuntime().onUserMessage(text: text, roomId: roomId)
    }

    func renderMessage(
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<String, Error> {
        await currentRuntime().renderMessage(messageId: messageId, messageType: messageType, messageData: messageData)
    }

    func dispatchEvent(roomId: String?, messageId: String, actionId: String, payload: String?) async {
        await currentRuntime().dispatchEvent(roomId: roomId, messageId: messageId, actionId: actionId, payload: payload)
    }

    @MainActor
    func attach(presentationView: ControllerBackedProtocol) {
        let runtime = state.withLock { state in
            state.presentationView = presentationView
            return state.runtime
        }

        runtime.attach(presentationView: presentationView)
    }

    func dispose() async {
        let runtime = state.withLock { state in
            state.disposed = true
            return state.runtime
        }

        await runtime.dispose()
    }
}

private extension ReloadableChatRuntime {
    func currentRuntime() -> ChatRuntimeProtocol {
        state.withLock { $0.runtime }
    }
}
