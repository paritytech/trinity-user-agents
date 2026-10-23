import Foundation
import Testing
import UIKit
import UIKitExt
@testable import polkadot_app

@MainActor
struct ReloadableChatRuntimeTests {
    private struct Unavailable: Error {}

    @Test func reloadMovesTheChatOntoTheNewRuntime() async throws {
        let current = MockChatRuntime()
        let replacement = MockChatRuntime()
        let runtime = ReloadableChatRuntime(runtime: current) { replacement }
        let view = StubPresentationView()

        try await runtime.start(messagingSupport: ProductsNativeApi.MessagingSupport(bot: nil, context: nil))
        runtime.attach(presentationView: view)

        try await runtime.reload()
        try await runtime.onUserMessage(text: "hello", roomId: nil)

        #expect(current.disposeCount == 1)
        #expect(replacement.startCount == 1)
        #expect(replacement.attachCount == 1)
        #expect(replacement.userMessages == ["hello"])
        #expect(current.userMessages.isEmpty)

        withExtendedLifetime(view) {}
    }

    @Test func reloadBeforeTheChatStartedDoesNotStartTheReplacement() async throws {
        let replacement = MockChatRuntime()
        let runtime = ReloadableChatRuntime(runtime: MockChatRuntime()) { replacement }

        try await runtime.reload()

        #expect(replacement.startCount == 0)
    }

    @Test func failedReloadKeepsTheCurrentRuntime() async throws {
        let current = MockChatRuntime()
        let runtime = ReloadableChatRuntime(runtime: current) { throw Unavailable() }

        await #expect(throws: Unavailable.self) { try await runtime.reload() }
        try await runtime.onUserMessage(text: "hello", roomId: nil)

        #expect(current.disposeCount == 0)
        #expect(current.userMessages == ["hello"])
    }

    @Test func reloadAfterDisposeDiscardsTheReplacement() async throws {
        let replacement = MockChatRuntime()
        let runtime = ReloadableChatRuntime(runtime: MockChatRuntime()) { replacement }

        await runtime.dispose()
        try await runtime.reload()

        #expect(replacement.disposeCount == 1)
        #expect(replacement.startCount == 0)
    }
}

@MainActor
private final class StubPresentationView: ControllerBackedProtocol {
    let controller = UIViewController()
    var isSetup: Bool { true }
}
