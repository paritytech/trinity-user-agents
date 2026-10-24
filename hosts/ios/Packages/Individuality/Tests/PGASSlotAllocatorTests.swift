import Foundation
import SubstrateSdk
import ExtrinsicService
import Testing
@testable import Individuality

struct PGASSlotAllocatorTests {
    private let slot = PGASSlotInfo(day: 7, slotIndex: 3, personOrigin: .lite(0, FakeBandersnatchKeyManaging()))

    private func makeAllocator(originFactory: FakePGasOriginFactory, submitter: FakeSubmitter) -> PGASSlotAllocator {
        PGASSlotAllocator(
            submissionChainId: "asset-hub",
            originChainId: "people",
            originFactory: originFactory,
            submitter: submitter,
            slotInfoProvider: FakePGASSlotInfoProvider(slot: slot)
        )
    }

    @Test func revisionFailureRebuildsTheOriginOnce() async throws {
        let originFactory = FakePGasOriginFactory(failures: [PGasOriginError.revisionTimeout(90)])
        let submitter = FakeSubmitter()

        let day = try await makeAllocator(originFactory: originFactory, submitter: submitter)
            .assignSlot(accountId: Data([0x01]), priority: .normal)

        #expect(day == 7)
        #expect(originFactory.calls == 2)
        #expect(submitter.submitCalls == ["asset-hub"])
    }

    @Test func secondRevisionFailureIsReported() async {
        let originFactory = FakePGasOriginFactory(failures: [
            PGasOriginError.revisionPruned(90),
            PGasOriginError.revisionPruned(91)
        ])
        let allocator = makeAllocator(originFactory: originFactory, submitter: FakeSubmitter())

        await #expect(throws: PGasOriginError.self) {
            try await allocator.assignSlot(accountId: Data([0x01]), priority: .normal)
        }
        #expect(originFactory.calls == 2)
    }

    @Test func submissionFailureIsNotRetried() async {
        let originFactory = FakePGasOriginFactory(failures: [])
        let submitter = FakeSubmitter()
        submitter.failureSequence = [true]
        let allocator = makeAllocator(originFactory: originFactory, submitter: submitter)

        await #expect(throws: (any Error).self) {
            try await allocator.assignSlot(accountId: Data([0x01]), priority: .normal)
        }
        #expect(originFactory.calls == 1)
    }
}

private final class FakePGasOriginFactory: PGasOriginCreating, @unchecked Sendable {
    private var failures: [Error]
    private(set) var calls = 0

    init(failures: [Error]) {
        self.failures = failures
    }

    func createPGASOrigin(
        personOrigin _: PersonOrigin,
        day _: UInt32,
        slotIndex _: UInt32,
        peopleChainId _: ChainId,
        submissionChainId _: ChainId
    ) async throws -> ExtrinsicOriginDefining {
        calls += 1
        if !failures.isEmpty {
            throw failures.removeFirst()
        }
        return StubExtrinsicOrigin()
    }
}

private struct FakePGASSlotInfoProvider: PGASSlotInfoProviding {
    let slot: PGASSlotInfo

    func hasExistingSlot(for _: AccountId) async throws -> Bool { false }
    func freeSlot() async throws -> PGASSlotInfo { slot }
}
