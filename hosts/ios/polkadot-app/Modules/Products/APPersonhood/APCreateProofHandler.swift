import struct TrUAPIHost.ProductId
import Foundation
import Individuality
import KeyDerivation
import Products
import SubstrateSdk

/// RFC-0004 Accounts Protocol `create_account_proof` backed by host-held member keys.
protocol APCreateProofHandling: Sendable {
    func createProof(
        keyHandle: RingVrfKeyHandle,
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProof
}

final class APCreateProofHandler: APBaseMembershipHandler, @unchecked Sendable {
    private let confirmationRequester: CreateProofConfirming

    init(
        callingProductId: ProductId?,
        builtInKeys: BuiltInPersonhoodKeysProviding,
        depsResolver: MembershipDepsResolving,
        confirmationRequester: CreateProofConfirming,
        logger: LoggerProtocol
    ) {
        self.confirmationRequester = confirmationRequester

        super.init(
            callingProductId: callingProductId,
            builtInKeys: builtInKeys,
            depsResolver: depsResolver,
            logger: logger
        )
    }
}

extension APCreateProofHandler: APCreateProofHandling {
    func createProof(
        keyHandle: RingVrfKeyHandle,
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProof {
        do {
            try await confirmProofCreationIfNeeded(context: context, message: message)
            let keyManager = try resolveKeyManager(handle: keyHandle, ring: ring)
            return try await performCreateProof(
                keyManager: keyManager,
                context: context,
                ring: ring,
                message: message
            )
        } catch {
            logger.error("Create proof failed: \(error)")
            throw CreateProofError.wrapping(error)
        }
    }
}

private extension APCreateProofHandler {
    func performCreateProof(
        keyManager: BandersnatchKeyManaging,
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProof {
        let contextBytes = try context.contextBytes()

        guard let collectionId = ring.collectionId else {
            throw CreateProofError.keyNotInRing
        }

        guard let deps = try await resolveDeps(for: ring) else {
            throw CreateProofError.ringNotFound
        }

        let blockHash = try await deps.blockInfoProvider.fetchCurrentHash()

        let ringIndex = try await fetchRingIndex(
            of: keyManager.getMemberKey(),
            in: collectionId,
            deps: deps,
            blockHash: blockHash
        )

        guard let params = try await deps.proofParamsFetcher.fetch(
            for: ringIndex,
            collectionId: collectionId,
            blockHash: blockHash
        ) else {
            throw CreateProofError.ringNotFound
        }

        guard let revision = try await deps.proofParamsFetcher.fetchCurrentRevision(
            for: ringIndex,
            collectionId: collectionId,
            blockHash: blockHash
        ) else {
            throw CreateProofError.ringNotFound
        }

        let proof = try keyManager.createProof(
            message,
            members: params.ringMembers,
            context: contextBytes,
            domainSize: params.ringSize
        )

        let alias = try keyManager.deriveAlias(for: contextBytes)

        return RingVrfProof(
            proof: proof,
            contextualAlias: ContextualAlias(context: contextBytes, alias: alias),
            ringIndex: ringIndex,
            ringRevision: revision
        )
    }

    // A cross-product proof is a one-time action: always prompt, never persist.
    func confirmProofCreationIfNeeded(context: ProductProofContext, message: Data) async throws {
        if let callingProductId, callingProductId == context.productId {
            return
        }

        let request = try CreateProofConfirmationRequest(
            callingProductId: callingProductId,
            onBehalfOfProductId: context.productId,
            suffix: context.suffix.index32().bytes,
            message: message
        )

        guard await confirmationRequester.confirm(request) == .approved else {
            throw CreateProofError.rejected
        }
    }

    func resolveKeyManager(handle: RingVrfKeyHandle, ring: RingLocation) throws -> BandersnatchKeyManaging {
        do {
            return try builtInKeys.resolve(handle: handle, ring: ring)
        } catch BuiltInPersonhoodKeyError.keyNotRegistered {
            throw CreateProofError.keyNotRegistered
        } catch BuiltInPersonhoodKeyError.keyNotInRing {
            throw CreateProofError.keyNotInRing
        }
    }

    func fetchRingIndex(
        of memberKey: BandersnatchPubKey,
        in collectionId: MembersPallet.CollectionIdentifier,
        deps: MembershipDeps,
        blockHash: BlockHashData?
    ) async throws -> MembersPallet.RingIndex {
        let statuses = try await deps.statusChecker.checkStatuses(
            of: [MembershipStatusInput(memberKey: memberKey, collection: collectionId)],
            blockHash: blockHash
        )

        guard let ringIndex = statuses[memberKey] else {
            throw CreateProofError.notMember
        }

        return ringIndex
    }
}
