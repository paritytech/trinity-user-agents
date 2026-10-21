import Foundation
import Testing
import UIKit
import UIKitExt
@testable import Products

/// O33: a prompt that no screen answers ends the request with an error, and nothing keeps it as a refusal.
@Suite("Unanswered permission prompts")
@MainActor
struct UnansweredPermissionPromptTests {
    private let productId = "test-product"

    /// A router that shows nothing and keeps the context of each prompt, so a test can answer or abandon it.
    private final class ScriptedRouter: ProductPermissionRouting {
        enum Script {
            case answer(PermissionDecision)
            case abandon(ProductPermissionPromptError)
            case keep
        }

        var script: Script
        private(set) var shown: [ProductPermissionContext] = []

        init(script: Script) {
            self.script = script
        }

        var isReady: Bool { true }

        func setPresentationView(_: ControllerBackedProtocol) {}

        func present(view _: ControllerBackedProtocol) -> Bool { true }

        func showPrompt(context: ProductPermissionContext) {
            shown.append(context)
            switch script {
            case let .answer(decision): context.deliver(decision)
            case let .abandon(error): context.abandon(error)
            case .keep: break
            }
        }
    }

    private struct NeverInForeground: ProductPromptForegroundWaiting {
        func waitForForeground(timeout _: Duration) async -> Bool { false }
    }

    // MARK: - Requester

    @Test("an answered prompt gives the decision")
    func answeredPromptGivesTheDecision() async throws {
        let router = ScriptedRouter(script: .answer(.allowOnce))
        let requester = ProductPermissionRequester(router: router)

        let decision = try await requester.prompt(productId: productId, permission: .preimageSubmitAccess)

        #expect(decision == .allowOnce)
        #expect(router.shown.count == 1)
    }

    @Test("a prompt that cannot show ends with cannotShow", arguments: [
        ProductPermissionPromptError.cannotShow,
        .closedWithoutAnswer
    ])
    func promptWithNoAnswerThrows(error: ProductPermissionPromptError) async {
        let router = ScriptedRouter(script: .abandon(error))
        let requester = ProductPermissionRequester(router: router)

        await #expect(throws: error) {
            try await requester.prompt(productId: productId, permission: .preimageSubmitAccess)
        }
    }

    @Test("a request waits for the foreground only for a bounded time, and shows no prompt")
    func backgroundRequestFailsWithoutAPrompt() async {
        let router = ScriptedRouter(script: .answer(.allowAlways))
        let requester = ProductPermissionRequester(
            router: router,
            foreground: NeverInForeground(),
            foregroundWait: .milliseconds(10)
        )

        await #expect(throws: ProductPermissionPromptError.notInForeground) {
            try await requester.promptBatched(productId: productId, permissions: [.preimageSubmitAccess])
        }
        #expect(router.shown.isEmpty)
    }

    // MARK: - Context

    @Test("the first end of a prompt wins")
    func firstEndWins() async throws {
        let context = ProductPermissionContext(productId: productId, permission: .webRtcAccess)

        let decision = try await withCheckedThrowingContinuation { continuation in
            context.setContinuation(continuation)
            context.deliver(.deny)
            context.abandon(.closedWithoutAnswer)
            context.deliver(.allowAlways)
        }

        #expect(decision == .deny)
        #expect(context.isAnswered)
    }

    @Test("a prompt that ended before the wait still ends the wait")
    func abandonBeforeTheWait() async {
        let context = ProductPermissionContext(productId: productId, permission: .webRtcAccess)
        context.abandon(.cannotShow)

        await #expect(throws: ProductPermissionPromptError.self) {
            try await withCheckedThrowingContinuation { continuation in
                context.setContinuation(continuation)
            }
        }
    }

    // MARK: - Handlers and guard

    @Test("a remote request with no answer throws and keeps no refusal")
    func remoteHandlerKeepsNoRefusal() async throws {
        let repository = MockProductPermissionRepository()
        let requester = MockProductPermissionRequester()
        requester.failure = .cannotShow
        let handler = RemotePermissionHandler(repository: repository, requester: requester)

        await #expect(throws: ProductPermissionPromptError.cannotShow) {
            try await handler.request(productId: productId, permission: .preimageSubmitAccess)
        }
        #expect(repository.denyCalls.isEmpty)
        let state = try await repository.getPermissionState(productId: productId, permission: .preimageSubmitAccess)
        #expect(state == .notDetermined)
    }

    @Test("the decision for the core throws, and the Bool request gives false; neither keeps a refusal")
    func guardMapsNoAnswer() async throws {
        let repository = MockProductPermissionRepository()
        let requester = MockProductPermissionRequester()
        requester.failure = .closedWithoutAnswer
        let sut = makeGuard(repository: repository, requester: requester, osAsker: MockOSPermissionAsker())

        await #expect(throws: ProductPermissionPromptError.closedWithoutAnswer) {
            try await sut.requestPermissionsDecision(productId: productId, permissions: [.preimageSubmitAccess])
        }
        #expect(try await !sut.requestPermission(productId: productId, permission: .preimageSubmitAccess))
        #expect(try await !sut.requestPermissionsBatched(productId: productId, permissions: [.webRtcAccess]))
        #expect(repository.denyCalls.isEmpty)
    }

    @Test("an OS permission that the OS gave already needs no screen")
    func grantedOsPermissionNeedsNoScreen() async throws {
        let repository = MockProductPermissionRepository()
        repository.stubState(productId: productId, permission: .deviceCapability(.camera), state: .allowedAlways)
        let osAsker = MockOSPermissionAsker()
        osAsker.checkResult = .allowed
        let handler = DeviceCapabilityPermissionHandler(
            repository: repository,
            requester: MockProductPermissionRequester(),
            osAsker: osAsker,
            foreground: NeverInForeground(),
            foregroundWait: .milliseconds(10)
        )

        let decision = try await handler.requestDecision(productId: productId, capability: .camera)

        #expect(decision == .allowAlways)
        #expect(osAsker.requestCalls.isEmpty)
    }

    @Test("an OS dialog with no foreground throws and keeps no refusal")
    func osDialogWithNoForegroundThrows() async throws {
        let repository = MockProductPermissionRepository()
        repository.stubState(productId: productId, permission: .deviceCapability(.notifications), state: .allowedAlways)
        let osAsker = MockOSPermissionAsker()
        osAsker.checkResult = .notDetermined
        let handler = DeviceCapabilityPermissionHandler(
            repository: repository,
            requester: MockProductPermissionRequester(),
            osAsker: osAsker,
            foreground: NeverInForeground(),
            foregroundWait: .milliseconds(10)
        )

        await #expect(throws: ProductPermissionPromptError.notInForeground) {
            try await handler.requestDecision(productId: productId, capability: .notifications)
        }
        #expect(osAsker.requestCalls.isEmpty)
        #expect(repository.denyCalls.isEmpty)
    }

    private func makeGuard(
        repository: MockProductPermissionRepository,
        requester: MockProductPermissionRequester,
        osAsker: MockOSPermissionAsker
    ) -> ProductPermissionGuard {
        ProductPermissionGuard(
            networkHandler: NetworkAccessPermissionHandler(repository: repository, requester: requester),
            remoteHandler: RemotePermissionHandler(repository: repository, requester: requester),
            deviceHandler: DeviceCapabilityPermissionHandler(
                repository: repository,
                requester: requester,
                osAsker: osAsker
            ),
            accountHandler: AccountAccessPermissionHandler(repository: repository, requester: requester),
            repository: repository,
            requester: requester
        )
    }
}
