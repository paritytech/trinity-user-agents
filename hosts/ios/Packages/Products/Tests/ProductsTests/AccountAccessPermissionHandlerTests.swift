import Foundation
import Testing
@testable import Products

@Suite("AccountAccessPermissionHandler Tests")
struct AccountAccessPermissionHandlerTests {
    private let productId = "requester-product"
    private let targetProductId = "target-product"

    private func makeSUT(
        decision: PermissionDecision = .allowAlways
    ) -> (
        handler: AccountAccessPermissionHandler,
        repository: MockProductPermissionRepository,
        requester: MockProductPermissionRequester
    ) {
        let repository = MockProductPermissionRepository()
        let requester = MockProductPermissionRequester()
        requester.decision = decision
        let handler = AccountAccessPermissionHandler(
            repository: repository,
            requester: requester
        )
        return (handler, repository, requester)
    }

    // MARK: - isGranted (same product)

    @Test("isGranted returns true for same-product access")
    func isGrantedSameProduct() async throws {
        let (handler, _, _) = makeSUT()

        let result = try await handler.isGranted(
            productId: productId,
            targetProductId: productId
        )

        #expect(result)
    }

    // MARK: - isGranted (cross product)

    @Test("isGranted returns false for notDetermined cross-product")
    func isGrantedCrossProductNotDetermined() async throws {
        let (handler, _, _) = makeSUT()

        let result = try await handler.isGranted(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(!result)
    }

    @Test("isGranted returns true for allowedAlways cross-product")
    func isGrantedCrossProductAllowed() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .accountAccess(targetProductId: targetProductId),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(result)
    }

    @Test("isGranted returns true for allowedOnce cross-product")
    func isGrantedCrossProductAllowedOnce() async throws {
        let (handler, repository, _) = makeSUT()
        repository.grantOneTime(
            productId: productId,
            permission: .accountAccess(targetProductId: targetProductId)
        )

        let result = try await handler.isGranted(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(result)
    }

    @Test("isGranted returns false for denied cross-product")
    func isGrantedCrossProductDenied() async throws {
        let (handler, repository, _) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .accountAccess(targetProductId: targetProductId),
            state: .denied
        )

        let result = try await handler.isGranted(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(!result)
    }

    // MARK: - request (same product)

    @Test("request returns true for same-product without prompting")
    func requestSameProduct() async throws {
        let (handler, _, requester) = makeSUT()

        let result = try await handler.request(
            productId: productId,
            targetProductId: productId
        )

        #expect(result)
        #expect(requester.promptCalls.isEmpty)
    }

    // MARK: - request (already resolved)

    @Test("request returns true immediately for allowedAlways")
    func requestAlreadyAllowed() async throws {
        let (handler, repository, requester) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .accountAccess(targetProductId: targetProductId),
            state: .allowedAlways
        )

        let result = try await handler.request(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(result)
        #expect(requester.promptCalls.isEmpty)
    }

    @Test("request returns false immediately for denied")
    func requestDenied() async throws {
        let (handler, repository, requester) = makeSUT()
        repository.stubState(
            productId: productId,
            permission: .accountAccess(targetProductId: targetProductId),
            state: .denied
        )

        let result = try await handler.request(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(!result)
        #expect(requester.promptCalls.isEmpty)
    }

    // MARK: - request (prompt flows)

    @Test("request prompts and persists grant for allowAlways")
    func requestPromptAllowAlways() async throws {
        let (handler, repository, requester) = makeSUT(decision: .allowAlways)

        let result = try await handler.request(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantCalls.count == 1)
        #expect(
            repository.grantCalls.first?.permission
                == .accountAccess(targetProductId: targetProductId)
        )
    }

    @Test("request prompts and grants one-time for allowOnce")
    func requestPromptAllowOnce() async throws {
        let (handler, repository, requester) = makeSUT(decision: .allowOnce)

        let result = try await handler.request(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantOneTimeCalls.count == 1)
        #expect(repository.grantCalls.isEmpty)
    }

    @Test("request prompts and persists deny for deny decision")
    func requestPromptDeny() async throws {
        let (handler, repository, requester) = makeSUT(decision: .deny)

        let result = try await handler.request(
            productId: productId,
            targetProductId: targetProductId
        )

        #expect(!result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.denyCalls.count == 1)
        #expect(
            repository.denyCalls.first?.permission
                == .accountAccess(targetProductId: targetProductId)
        )
    }
}
