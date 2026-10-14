import Foundation
import SubstrateSdk
import KeyDerivation
import ChainStore
import StructuredConcurrency

public final class PGASSlotAllocator: AllowanceSlotAllocating {
    private let submissionChainId: ChainId
    private let originChainId: ChainId
    private let originFactory: PGasOriginCreating
    private let submitter: SlotAssignmentSubmitting
    private let slotInfoProvider: PGASSlotInfoProviding

    public init(
        submissionChainId: ChainId,
        originChainId: ChainId,
        originFactory: PGasOriginCreating,
        submitter: SlotAssignmentSubmitting,
        slotInfoProvider: PGASSlotInfoProviding
    ) {
        self.submissionChainId = submissionChainId
        self.originChainId = originChainId
        self.originFactory = originFactory
        self.submitter = submitter
        self.slotInfoProvider = slotInfoProvider
    }

    public func assignSlot(accountId: AccountId, priority _: AllowanceRecord.Priority) async throws -> UInt32 {
        try await markStallActivity("Reserving PGas slot") {
            let slot = try await markStallRegion("Reading chain state") {
                try await self.slotInfoProvider.freeSlot()
            }

            try await markStallRegion("Submitting") {
                try await self.submitter.submit(
                    call: PGASPallet.ClaimPgasCall(
                        slotIndex: slot.slotIndex,
                        target: accountId
                    )(),
                    makeOrigin: { [
                        originFactory = self.originFactory,
                        slot,
                        originChainId = self.originChainId,
                        submissionChainId = self.submissionChainId
                    ] _ in
                        try await originFactory.createPGASOrigin(
                            personOrigin: slot.personOrigin,
                            day: slot.day,
                            slotIndex: slot.slotIndex,
                            peopleChainId: originChainId,
                            submissionChainId: submissionChainId
                        )
                    },
                    chainId: self.submissionChainId
                )
            }

            return slot.day
        }
    }
}
