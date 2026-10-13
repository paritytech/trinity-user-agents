import Foundation
import Individuality
import Testing
@testable import polkadot_app

struct ChatRequestDeliveryAccountResolverTests {
    private let requestId = "request"
    private let allocator = StubNotificationAllocator()
    private let signers =
        ChatRequestDeliverySigners(signManager: ChatSignerManager(entropyManager: FixedRootEntropyManager()))

    @Test func signsWithPeriodAccountOnceItsSlotIsClaimed() async throws {
        let expected = try signers.anonymous(requestId: requestId, period: allocator.period)

        let resolved = try await makeResolver().resolveFirstDelivery(requestId: requestId)

        #expect(resolved.period == allocator.period)
        #expect(resolved.signer.accountId == expected.signer.accountId)
        #expect(allocator.initiatedTargets == [expected.signer.accountId])
    }

    @Test func keepsRequestPendingWhenNoSlotIsFree() async throws {
        allocator.hasFreeSlot = false

        await #expect(throws: NotificationAllocationError.noFreeSlotInPeriod) {
            try await makeResolver().resolveFirstDelivery(requestId: requestId)
        }
    }

    @Test func failsWhenClaimDidNotLandInTime() async throws {
        allocator.awaitResult = .failure(NotificationAllocationError.timeout)

        await #expect(throws: NotificationAllocationError.timeout) {
            try await makeResolver().resolveFirstDelivery(requestId: requestId)
        }
    }

    @Test func derivesDistinctAccountsPerRequestAndPeriod() throws {
        let first = try signers.anonymous(requestId: requestId, period: 100).signer.accountId

        #expect(try signers.anonymous(requestId: requestId, period: 101).signer.accountId != first)
        #expect(try signers.anonymous(requestId: "other", period: 100).signer.accountId != first)
    }

    private func makeResolver() -> ChatRequestDeliveryAccountResolver {
        ChatRequestDeliveryAccountResolver(allocator: allocator, signers: signers)
    }
}
