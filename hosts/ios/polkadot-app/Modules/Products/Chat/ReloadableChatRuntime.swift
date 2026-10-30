import Foundation
import os
import Products
import UIKitExt

protocol ReloadableChatRuntimeProtocol: ChatRuntimeProtocol {
    func reload() async throws
}

final class ReloadableChatRuntime: ReloadableChatRuntimeProtocol, @unchecked Sendable {
    private struct State {
        var runtime: ChatRuntimeProtocol
        var messagingSupport: ProductsNativeApi.MessagingSupport?
        weak var presentationView: ControllerBackedProtocol?
        var disposed = false
    }

    private let productId: ProductId
    private let runtimeFactory: ChatRuntimeMaking
    private let contentLoader: ProductContentLoading
    private let state: OSAllocatedUnfairLock<State>

    init(
        resolved: ResolvedProduct,
        runtimeFactory: ChatRuntimeMaking,
        contentLoader: ProductContentLoading
    ) throws {
        productId = resolved.id
        self.runtimeFactory = runtimeFactory
        self.contentLoader = contentLoader
        state = try OSAllocatedUnfairLock(uncheckedState: State(runtime: runtimeFactory.makeRuntime(for: resolved)))
    }

    func reload() async throws {
        let latest = try await contentLoader.openWorker(productId)
        let replacement = try runtimeFactory.makeRuntime(for: latest)

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
        roomId: String?,
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        await currentRuntime().renderMessage(
            roomId: roomId,
            messageId: messageId,
            messageType: messageType,
            messageData: messageData
        )
    }

    func dispatchEvent(
        roomId: String?,
        messageId: String,
        messageType: String?,
        actionId: String,
        payload: String?
    ) async {
        await currentRuntime().dispatchEvent(
            roomId: roomId,
            messageId: messageId,
            messageType: messageType,
            actionId: actionId,
            payload: payload
        )
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
