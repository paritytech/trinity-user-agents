import Foundation
import SubstrateSdk
import KeyDerivation
import ChainStore
import StructuredConcurrency

public final class StatementStoreSlotAllocator: AllowanceSlotAllocating {
    private let chainId: ChainId
    private let originFactory: AsResourcesOriginCreating
    private let submitter: SlotAssignmentSubmitting
    private let slotInfoProvider: StatementStoreSlotInfoProviding
    private let serialQueue: SerialOperationQueue

    public init(
        chainId: ChainId,
        originFactory: AsResourcesOriginCreating,
        submitter: SlotAssignmentSubmitting,
        slotInfoProvider: StatementStoreSlotInfoProviding,
        serialQueue: SerialOperationQueue
    ) {
        self.chainId = chainId
        self.originFactory = originFactory
        self.submitter = submitter
        self.slotInfoProvider = slotInfoProvider
        self.serialQueue = serialQueue
    }

    public func assignSlot(accountId: AccountId, priority: AllowanceRecord.Priority) async throws -> UInt32 {
        try await markStallRegion("Reserving SS slot") {
            try await serialQueue.run { [weak self] in
                guard let self else { throw AllowanceSlotAssignmentError.noSlotsAvailable }

                let slot = try await markStallRegion("Reading chain state") {
                    try await self.slotInfoProvider.freeSlot(excluding: accountId, callerPriority: priority)
                }

                let callData = ResourcesPallet.SetStatementStoreAccountCall(
                    period: slot.period,
                    seq: slot.seq,
                    targetAccount: accountId
                )()

                try await markStallRegion("Submitting") {
                    try await self.submitter.submit(
                        call: callData,
                        makeOrigin: { [originFactory = self.originFactory, slot] chainIdParam in
                            try await originFactory.createSSSOrigin(
                                personOrigin: slot.personOrigin,
                                period: slot.period,
                                seq: slot.seq,
                                chain: chainIdParam
                            )
                        },
                        chainId: self.chainId
                    )
                }

                return slot.period
            }
        }
    }
}
