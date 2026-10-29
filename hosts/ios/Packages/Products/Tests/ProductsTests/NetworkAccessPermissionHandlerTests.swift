import Foundation
import Testing
@testable import Products

@Suite("NetworkAccessPermissionHandler Tests")
struct NetworkAccessPermissionHandlerTests {
    private let productId = "test-product"

    private func makeSUT(
        promptDecision: PermissionDecision = .allowAlways
    ) -> (
        handler: NetworkAccessPermissionHandler,
        repository: MockProductPermissionRepository,
        requester: MockProductPermissionRequester
    ) {
        let repository = MockProductPermissionRepository()
        let requester = MockProductPermissionRequester()
        requester.decision = promptDecision
        let handler = NetworkAccessPermissionHandler(
            repository: repository,
            requester: requester
        )
        return (handler, repository, requester)
    }

    // MARK: - isGranted

    @Test("isGranted returns false for notDetermined permission")
    func isGrantedNotDetermined() async throws {
        let (handler, _, _) = makeSUT()

        let result = try await handler.isGranted(
            productId: productId,
            domain: "unknown.com"
        )

        #expect(!result)
    }

    @Test("isGranted returns true for allowedAlways permission")
    func isGrantedAllowedAlways() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .networkAccess(domain: "example.com"),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            domain: "example.com"
        )

        #expect(result)
    }

    @Test("isGranted returns true for allowedOnce permission")
    func isGrantedAllowedOnce() async throws {
        let (handler, repository, _) = makeSUT()
        repository.grantOneTime(
            productId: productId,
            permission: .networkAccess(domain: "example.com")
        )

        let result = try await handler.isGranted(
            productId: productId,
            domain: "example.com"
        )

        #expect(result)
    }

    @Test("isGranted returns false for denied permission")
    func isGrantedDenied() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .networkAccess(domain: "evil.com"),
            state: .denied
        )

        let result = try await handler.isGranted(
            productId: productId,
            domain: "evil.com"
        )

        #expect(!result)
    }

    // MARK: - isGranted (allowed domains)

    @Test("isGranted returns true for globally allowed domain")
    func isGrantedAllowedDomain() async throws {
        let (handler, _, _) = makeSUT()

        let result = try await handler.isGranted(
            productId: productId,
            domain: "fonts.googleapis.com"
        )

        #expect(result)
    }

    // MARK: - isGranted (subdomain matching)

    @Test("isGranted returns true when wildcard parent domain is allowed")
    func isGrantedSubdomainMatch() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .networkAccess(domain: "*.example.com"),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            domain: "api.example.com"
        )

        #expect(result)
    }

    @Test("isGranted returns false for subdomain when bare parent is allowed")
    func isGrantedBareParentDoesNotMatchSubdomain() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .networkAccess(domain: "example.com"),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            domain: "api.example.com"
        )

        #expect(!result)
    }

    @Test("isGranted returns true for deep subdomain when wildcard root is allowed")
    func isGrantedDeepSubdomainMatch() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .networkAccess(domain: "*.example.com"),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            domain: "deep.api.example.com"
        )

        #expect(result)
    }

    // MARK: - request (prompt flows)

    @Test("request returns true immediately for already allowed domain")
    func requestAlreadyAllowed() async throws {
        let (handler, repository, requester) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .networkAccess(domain: "example.com"),
            state: .allowedAlways
        )

        let result = try await handler.request(
            productId: productId,
            domain: "example.com"
        )

        #expect(result)
        #expect(requester.promptCalls.isEmpty)
    }

    @Test("request returns false immediately for denied domain")
    func requestDenied() async throws {
        let (handler, repository, requester) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .networkAccess(domain: "evil.com"),
            state: .denied
        )

        let result = try await handler.request(
            productId: productId,
            domain: "evil.com"
        )

        #expect(!result)
        #expect(requester.promptCalls.isEmpty)
    }

    @Test("request prompts and persists grant for allowAlways")
    func requestPromptAllowAlways() async throws {
        let (handler, repository, requester) = makeSUT(promptDecision: .allowAlways)

        let result = try await handler.request(
            productId: productId,
            domain: "new.com"
        )

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantCalls.count == 1)
        #expect(repository.grantCalls.first?.permission == .networkAccess(domain: "new.com"))
    }

    @Test("request prompts and grants one-time for allowOnce")
    func requestPromptAllowOnce() async throws {
        let (handler, repository, requester) = makeSUT(promptDecision: .allowOnce)

        let result = try await handler.request(
            productId: productId,
            domain: "new.com"
        )

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantOneTimeCalls.count == 1)
        #expect(repository.grantCalls.isEmpty)
    }

    @Test("request prompts and persists deny for deny decision")
    func requestPromptDeny() async throws {
        let (handler, repository, requester) = makeSUT(promptDecision: .deny)

        let result = try await handler.request(
            productId: productId,
            domain: "new.com"
        )

        #expect(!result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.denyCalls.count == 1)
        #expect(repository.denyCalls.first?.permission == .networkAccess(domain: "new.com"))
    }

    private func makeGuard(
        repository: MockProductPermissionRepository,
        requester: MockProductPermissionRequester
    ) -> ProductPermissionGuard {
        ProductPermissionGuard(
            networkHandler: NetworkAccessPermissionHandler(repository: repository, requester: requester),
            remoteHandler: RemotePermissionHandler(repository: repository, requester: requester),
            deviceHandler: DeviceCapabilityPermissionHandler(
                repository: repository, requester: requester, osAsker: MockOSPermissionAsker()
            ),
            accountHandler: AccountAccessPermissionHandler(repository: repository, requester: requester),
            repository: repository, requester: requester
        )
    }

    @Test(arguments: [PermissionDecision.allowOnce, .allowAlways, .deny])
    func decisionPreservesConsentWithoutDuplicatingGrants(decision: PermissionDecision) async throws {
        let (_, repository, requester) = makeSUT(promptDecision: decision)
        let guardService = makeGuard(repository: repository, requester: requester)
        let permission = ProductPermission.networkAccess(domain: "api.example.com")
        let result = try await guardService.requestPermissionsDecision(productId: productId, permissions: [permission])
        #expect(result == decision)
        #expect(try await repository.getPermissionState(productId: productId, permission: permission) == .notDetermined)
    }

    @Test
    func decisionRequestsFreshConsentWithoutTransferringLegacyOneTimeGrants() async throws {
        let (_, repository, requester) = makeSUT(promptDecision: .allowAlways)
        let guardService = makeGuard(repository: repository, requester: requester)
        let temporary = ProductPermission.networkAccess(domain: "once.example.com")
        let permanent = ProductPermission.networkAccess(domain: "always.example.com")
        let newPermission = ProductPermission.networkAccess(domain: "new.example.com")
        repository.grantOneTime(productId: productId, permission: temporary)
        try await repository.grant(productId: productId, permission: permanent)
        let result = try await guardService.requestPermissionsDecision(
            productId: productId, permissions: [temporary, permanent, newPermission]
        )
        #expect(result == .allowAlways)
        #expect(requester.promptBatchedCalls.map(\.permissions) == [[temporary, newPermission]])
        let states = try await [
            repository.getPermissionState(productId: productId, permission: temporary),
            repository.getPermissionState(productId: productId, permission: permanent),
            repository.getPermissionState(productId: productId, permission: newPermission)
        ]
        #expect(states == [.allowedOnce, .allowedAlways, .notDetermined])
    }
}
