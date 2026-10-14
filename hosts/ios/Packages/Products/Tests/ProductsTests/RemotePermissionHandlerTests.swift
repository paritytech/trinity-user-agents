import Foundation
import Testing
@testable import Products

@Suite("RemotePermissionHandler Tests")
struct RemotePermissionHandlerTests {
    private let productId = "test-product"
    private let permission = ProductPermission.webRtcAccess

    private func makeSUT(
        promptDecision: PermissionDecision = .allowAlways
    ) -> (
        handler: RemotePermissionHandler,
        repository: MockProductPermissionRepository,
        requester: MockProductPermissionRequester
    ) {
        let repository = MockProductPermissionRepository()
        let requester = MockProductPermissionRequester()
        requester.decision = promptDecision
        let handler = RemotePermissionHandler(
            repository: repository,
            requester: requester
        )
        return (handler, repository, requester)
    }

    // MARK: - isGranted

    @Test("isGranted returns false for notDetermined webRtc permission")
    func isGrantedNotDetermined() async throws {
        let (handler, _, _) = makeSUT()

        let result = try await handler.isGranted(productId: productId, permission: permission)

        #expect(!result)
    }

    @Test("isGranted returns true for allowedAlways webRtc permission")
    func isGrantedAllowedAlways() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(productId: productId, permission: permission, state: .allowedAlways)

        let result = try await handler.isGranted(productId: productId, permission: permission)

        #expect(result)
    }

    @Test("isGranted returns true for allowedOnce webRtc permission")
    func isGrantedAllowedOnce() async throws {
        let (handler, repository, _) = makeSUT()
        repository.grantOneTime(productId: productId, permission: permission)

        let result = try await handler.isGranted(productId: productId, permission: permission)

        #expect(result)
    }

    @Test("isGranted returns false for denied webRtc permission")
    func isGrantedDenied() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(productId: productId, permission: permission, state: .denied)

        let result = try await handler.isGranted(productId: productId, permission: permission)

        #expect(!result)
    }

    // MARK: - request (prompt flows)

    @Test("request returns true immediately when already allowed without prompting")
    func requestAlreadyAllowed() async throws {
        let (handler, repository, requester) = makeSUT()
        repository.stubState(productId: productId, permission: permission, state: .allowedAlways)

        let result = try await handler.request(productId: productId, permission: permission)

        #expect(result)
        #expect(requester.promptCalls.isEmpty)
    }

    @Test("request returns false immediately when denied without prompting")
    func requestDenied() async throws {
        let (handler, repository, requester) = makeSUT()
        repository.stubState(productId: productId, permission: permission, state: .denied)

        let result = try await handler.request(productId: productId, permission: permission)

        #expect(!result)
        #expect(requester.promptCalls.isEmpty)
    }

    @Test("request prompts and persists grant for allowAlways")
    func requestPromptAllowAlways() async throws {
        let (handler, repository, requester) = makeSUT(promptDecision: .allowAlways)

        let result = try await handler.request(productId: productId, permission: permission)

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantCalls.count == 1)
        #expect(repository.grantCalls.first?.permission == permission)
    }

    @Test("request prompts and grants one-time for allowOnce")
    func requestPromptAllowOnce() async throws {
        let (handler, repository, requester) = makeSUT(promptDecision: .allowOnce)

        let result = try await handler.request(productId: productId, permission: permission)

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantOneTimeCalls.count == 1)
        #expect(repository.grantCalls.isEmpty)
    }

    @Test("request prompts and persists deny for deny decision")
    func requestPromptDeny() async throws {
        let (handler, repository, requester) = makeSUT(promptDecision: .deny)

        let result = try await handler.request(productId: productId, permission: permission)

        #expect(!result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.denyCalls.count == 1)
        #expect(repository.denyCalls.first?.permission == permission)
    }
}
