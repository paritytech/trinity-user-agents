import Foundation
import SubstrateSdk
import KeyDerivation
import ChainStore
import ExtrinsicService
import StructuredConcurrency

public final class StatementStoreSlotAllocator: AllowanceSlotAllocating {
    /// `StmtStoreReplacementTooEarly`: the slot entry was written in the current block.
    static let tooEarlyValidityCode: UInt8 = 228

    public struct Dependencies {
        let originFactory: AsResourcesOriginCreating
        let submitter: SlotAssignmentSubmitting
        let slotInfoProvider: StatementStoreSlotInfoProviding
        let replacementValidator: SlotReplacementValidating
        let capabilities: StatementStoreRuntimeCapabilitiesProviding
        let resourcesParameters: ResourcesParametersProviding

        public init(
            originFactory: AsResourcesOriginCreating,
            submitter: SlotAssignmentSubmitting,
            slotInfoProvider: StatementStoreSlotInfoProviding,
            replacementValidator: SlotReplacementValidating,
            capabilities: StatementStoreRuntimeCapabilitiesProviding,
            resourcesParameters: ResourcesParametersProviding
        ) {
            self.originFactory = originFactory
            self.submitter = submitter
            self.slotInfoProvider = slotInfoProvider
            self.replacementValidator = replacementValidator
            self.capabilities = capabilities
            self.resourcesParameters = resourcesParameters
        }
    }

    private let chainId: ChainId
    private let dependencies: Dependencies
    private let serialQueue: SerialOperationQueue

    public init(chainId: ChainId, dependencies: Dependencies, serialQueue: SerialOperationQueue) {
        self.chainId = chainId
        self.dependencies = dependencies
        self.serialQueue = serialQueue
    }

    public func assignSlot(accountId: AccountId, priority: AllowanceRecord.Priority) async throws -> UInt32 {
        try await markStallRegion("Reserving SS slot") {
            try await serialQueue.run { [weak self] in
                guard let self else { throw AllowanceSlotAssignmentError.noSlotsAvailable }

                let plan = try await markStallRegion("Reading chain state") {
                    try await self.dependencies.slotInfoProvider.slotPlan(
                        excluding: accountId,
                        callerPriority: priority
                    )
                }

                switch plan {
                case let .free(slot):
                    try await claimFreeSlot(slot, accountId: accountId)
                    return slot.period
                case let .evict(slot):
                    try await evict(slot, for: accountId)
                    return slot.period
                }
            }
        }
    }
}

private extension StatementStoreSlotAllocator {
    func evict(_ slot: SSSEvictSlotInfo, for accountId: AccountId) async throws {
        if try await dependencies.capabilities.supportsSlotReplacement() {
            try await replaceSlot(period: slot.period, candidates: slot.candidates, accountId: accountId)
        } else {
            try await evictWithProof(
                period: slot.period,
                nowSeconds: slot.nowSeconds,
                candidates: slot.candidates,
                accountId: accountId
            )
        }
    }

    func claimFreeSlot(_ slot: SSSFreeSlotInfo, accountId: AccountId) async throws {
        try await submitSetCall(
            period: slot.period,
            seq: slot.seq,
            personOrigin: slot.personOrigin,
            targetAccount: accountId
        )
    }

    /// Old runtimes let the ring proof take an occupied slot once its cooldown has elapsed.
    func evictWithProof(
        period: UInt32,
        nowSeconds: UInt64,
        candidates: [SSSEvictionCandidate],
        accountId: AccountId
    ) async throws {
        let cooldown = try await dependencies.resourcesParameters.stmtStoreReplacementCooldown(chainId: chainId)

        guard let slot = LegacyEvictionPolicy.firstCooledDown(candidates, cooldown: cooldown, nowSeconds: nowSeconds)
        else {
            throw StatementStoreAllowanceError.noSlotsAvailable
        }

        try await submitSetCall(
            period: period,
            seq: slot.seq,
            personOrigin: slot.personOrigin,
            targetAccount: accountId
        )
    }

    func submitSetCall(period: UInt32, seq: UInt32, personOrigin: PersonOrigin, targetAccount: AccountId) async throws {
        let callData = ResourcesPallet.SetStatementStoreAccountCall(
            period: period,
            seq: seq,
            targetAccount: targetAccount
        )()

        try await markStallRegion("Submitting") {
            try await dependencies.submitter.submit(
                call: callData,
                makeOrigin: { [originFactory = dependencies.originFactory] chain in
                    try await originFactory.createSSSOrigin(
                        personOrigin: personOrigin,
                        period: period,
                        seq: seq,
                        chain: chain
                    )
                },
                chainId: chainId
            )
        }
    }

    /// New runtimes need the holder's signature, so only slots whose key this installation holds qualify.
    func replaceSlot(period: UInt32, candidates: [SSSEvictionCandidate], accountId: AccountId) async throws {
        var anyRateLimited = false

        for candidate in candidates {
            guard let origin = try await holderOrigin(for: candidate) else {
                continue
            }

            let callData = ResourcesPallet.ReplaceStatementStoreAccountCall(
                period: period,
                alias: candidate.alias,
                newAccount: accountId,
                since: candidate.since
            )()

            let validity = try await markStallRegion("Validating replacement") {
                try await dependencies.replacementValidator.validate(call: callData, origin: origin, chainId: chainId)
            }

            switch validity {
            case .invalid(.payment),
                 .invalid(.custom(Self.tooEarlyValidityCode)):
                anyRateLimited = true
                continue
            case let .invalid(reason):
                throw StatementStoreAllowanceError.replacementRejected(reason)
            case .valid,
                 .inconclusive:
                try await markStallRegion("Submitting") {
                    try await dependencies.submitter.submit(
                        call: callData,
                        makeOrigin: { _ in origin },
                        chainId: chainId
                    )
                }
                return
            }
        }

        throw anyRateLimited ? StatementStoreAllowanceError.noSlotsAvailable : .noEvictableSlots
    }

    /// A stored key that no longer derives the holder cannot sign the takeover, so the row counts as keyless.
    func holderOrigin(for candidate: SSSEvictionCandidate) async throws -> ExtrinsicOriginDefining? {
        guard let signerKeyId = candidate.signerKeyId else {
            return nil
        }

        do {
            return try await dependencies.originFactory.createStmtStoreHolderOrigin(
                signerKeyId: signerKeyId,
                holder: candidate.holder,
                chain: chainId
            )
        } catch is SlotHolderSignerError {
            return nil
        }
    }
}
