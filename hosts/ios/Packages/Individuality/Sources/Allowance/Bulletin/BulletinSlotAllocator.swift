import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import KeyDerivation
import FoundationExt
import ChainStore
import StructuredConcurrency

public final class BulletinSlotAllocator {
    private let submissionChainId: ChainId
    private let slotInfoProvider: BulletInSlotInfoProviding
    private let originFactory: AsResourcesOriginCreating
    private let submitter: SlotAssignmentSubmitting

    public init(
        submissionChainId: ChainId,
        slotInfoProvider: BulletInSlotInfoProviding,
        originFactory: AsResourcesOriginCreating,
        submitter: SlotAssignmentSubmitting
    ) {
        self.submissionChainId = submissionChainId
        self.slotInfoProvider = slotInfoProvider
        self.originFactory = originFactory
        self.submitter = submitter
    }
}

extension BulletinSlotAllocator: AllowanceSlotAllocating {
    public func assignSlot(accountId: AccountId, priority _: AllowanceRecord.Priority) async throws -> UInt32 {
        try await markStallActivity("Reserving Bulletin slot") {
            let slotInfo = try await markStallRegion("Reading chain state") {
                try await self.slotInfoProvider.fetchFreeSlotInfo()
            }

            try await markStallRegion("Submitting") {
                try await self.submitter.submit(
                    call: ResourcesPallet.ClaimLongTermStorageCall(
                        period: slotInfo.period,
                        counter: slotInfo.counter,
                        accountId: accountId
                    )(),
                    makeOrigin: { [originFactory = self.originFactory] chainId in
                        try await originFactory.createLTSOrigin(
                            personOrigin: slotInfo.personOrigin,
                            period: slotInfo.period,
                            counter: slotInfo.counter,
                            chain: chainId
                        )
                    },
                    chainId: self.submissionChainId
                )
            }

            return slotInfo.period
        }
    }
}
