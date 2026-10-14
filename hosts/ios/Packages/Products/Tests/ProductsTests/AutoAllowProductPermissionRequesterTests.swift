import Foundation
import Testing
@testable import Products

@Suite("AutoAllowProductPermissionRequester Tests")
struct AutoAllowProductPermissionRequesterTests {
    private let allowedLabels = Set(["getcash"])

    private func makeSUT(
        wrappedDecision: PermissionDecision = .deny
    ) -> (
        requester: AutoAllowProductPermissionRequester,
        wrapped: MockProductPermissionRequester
    ) {
        let wrapped = MockProductPermissionRequester()
        wrapped.decision = wrappedDecision
        let requester = AutoAllowProductPermissionRequester(
            allowedLabels: allowedLabels,
            wrapped: wrapped
        )
        return (requester, wrapped)
    }

    @Test("prompt auto-allows for allowlisted product")
    func promptAutoAllowsAllowlisted() async throws {
        let (requester, wrapped) = makeSUT()

        let decision = await requester.prompt(
            productId: "getcash.dot",
            permission: .networkAccess(domain: "example.com")
        )

        #expect(decision == .allowAlways)
        #expect(wrapped.promptCalls.isEmpty)
    }

    @Test("promptBatched auto-allows for allowlisted product")
    func promptBatchedAutoAllowsAllowlisted() async throws {
        let (requester, wrapped) = makeSUT()

        let decision = await requester.promptBatched(
            productId: "getcash.dot",
            permissions: [.balanceAccess, .webRtcAccess]
        )

        #expect(decision == .allowAlways)
        #expect(wrapped.promptBatchedCalls.isEmpty)
    }

    @Test("prompt delegates non-allowlisted product to wrapped")
    func promptDelegatesNonAllowlisted() async throws {
        let (requester, wrapped) = makeSUT(wrappedDecision: .deny)

        let decision = await requester.prompt(
            productId: "other.dot",
            permission: .balanceAccess
        )

        #expect(decision == .deny)
        #expect(wrapped.promptCalls.count == 1)
        #expect(wrapped.promptCalls.first?.productId == "other.dot")
    }

    @Test("prompt delegates bare productId without root")
    func promptDelegatesBareProductId() async throws {
        let (requester, wrapped) = makeSUT(wrappedDecision: .deny)

        let decision = await requester.prompt(
            productId: "getcash",
            permission: .balanceAccess
        )

        #expect(decision == .deny)
        #expect(wrapped.promptCalls.count == 1)
        #expect(wrapped.promptCalls.first?.productId == "getcash")
    }

    @Test("prompt delegates productId with an empty root label")
    func promptDelegatesEmptyRootLabel() async throws {
        let (requester, wrapped) = makeSUT(wrappedDecision: .deny)

        let decision = await requester.prompt(
            productId: "getcash..dot",
            permission: .balanceAccess
        )

        #expect(decision == .deny)
        #expect(wrapped.promptCalls.count == 1)
        #expect(wrapped.promptCalls.first?.productId == "getcash..dot")
    }
}
