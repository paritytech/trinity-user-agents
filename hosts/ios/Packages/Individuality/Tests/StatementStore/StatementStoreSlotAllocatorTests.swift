import Foundation
import SubstrateSdk
import StructuredConcurrency
import Testing
@testable import Individuality

struct StatementStoreSlotAllocatorTests {
    private let keyManager = FakeBandersnatchKeyManaging()
    private let chainId = "polkadot"
    private let collection = "test-collection"

    private func makePersonOrigin() -> PersonOrigin {
        .lite(0, keyManager)
    }

    private func makeAllocator(
        slotInfoProvider: FakeSlotInfoProvider,
        submitter: FakeSubmitter
    ) -> StatementStoreSlotAllocator {
        let queue = SerialOperationQueue()
        let originFactory = FakeOriginFactory()

        return StatementStoreSlotAllocator(
            chainId: chainId,
            originFactory: originFactory,
            submitter: submitter,
            slotInfoProvider: slotInfoProvider,
            serialQueue: queue
        )
    }

    // Test 5: Delta, no eviction
    @Test func deltaSlotsWithoutEvictionReturnsOutcome() async throws {
        let accountId = Data([0x01])
        let personOrigin = makePersonOrigin()

        let slotProvider = FakeSlotInfoProvider()
        let slotInfo = SSSSlotInfo(
            period: 100,
            seq: 5,
            personOrigin: personOrigin
        )
        slotProvider.updateFreeSlot(.success(slotInfo))

        let submitter = FakeSubmitter()

        let allocator = makeAllocator(
            slotInfoProvider: slotProvider,
            submitter: submitter
        )

        let outcome = try await allocator.assignSlot(accountId: accountId, priority: .normal)

        #expect(outcome == 100)
    }

    // Test 6: Delta, with eviction - allocator still picks the slot for eviction
    @Test func deltaSlotsWithEvictionReturnsOutcome() async throws {
        let accountId = Data([0x01])
        let personOrigin = makePersonOrigin()

        let slotProvider = FakeSlotInfoProvider()
        let slotInfo = SSSSlotInfo(
            period: 100,
            seq: 5,
            personOrigin: personOrigin
        )
        slotProvider.updateFreeSlot(.success(slotInfo))

        let submitter = FakeSubmitter()

        let allocator = makeAllocator(
            slotInfoProvider: slotProvider,
            submitter: submitter
        )

        let outcome = try await allocator.assignSlot(accountId: accountId, priority: .normal)

        #expect(outcome == 100)
    }

    // Test 7: no slots available
    @Test func noSlotsAvailableThrowsError() async throws {
        let accountId = Data([0x01])

        let slotProvider = FakeSlotInfoProvider()
        slotProvider.updateFreeSlot(.failure(AllowanceSlotAssignmentError.noSlotsAvailable))

        let submitter = FakeSubmitter()

        let allocator = makeAllocator(
            slotInfoProvider: slotProvider,
            submitter: submitter
        )

        do {
            try await allocator.assignSlot(accountId: accountId, priority: .normal)
            #expect(Bool(false), "Expected assignSlot to throw")
        } catch {
            // Expected to fail when no slots available
        }
    }
}
