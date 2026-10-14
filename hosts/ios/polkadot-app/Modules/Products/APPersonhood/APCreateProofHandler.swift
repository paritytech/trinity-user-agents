import Foundation
import Individuality
import KeyDerivation
import Products
import SubstrateSdk

/// RFC-0004 Accounts Protocol `create_account_proof` backed by host-held member keys.
protocol APCreateProofHandling: Sendable {
    func createProof(
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProof
}

final class APCreateProofHandler: APBaseMembershipHandler, @unchecked Sendable {
    private let confirmationRequester: CreateProofConfirming

    init(
        callingProductId: ProductId?,
        options: [CreateProofOrAliasOption],
        depsResolver: MembershipDepsResolving,
        confirmationRequester: CreateProofConfirming,
        logger: LoggerProtocol
    ) {
        self.confirmationRequester = confirmationRequester

        super.init(
            callingProductId: callingProductId,
            options: options,
            depsResolver: depsResolver,
            logger: logger
        )
    }
}

extension APCreateProofHandler: APCreateProofHandling {
    func createProof(
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProof {
        do {
            return try await performCreateProof(context: context, ring: ring, message: message)
        } catch {
            logger.error("Create proof failed: \(error)")
            throw CreateProofError.wrapping(error)
        }
    }
}

private extension APCreateProofHandler {
    func performCreateProof(
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProof {
        let contextBytes = try context.contextBytes()

        try await confirmProofCreationIfNeeded(context: context, message: message)

        guard let deps = try await resolveDeps(for: ring) else {
            throw CreateProofError.ringNotFound
        }

        let blockHash = try await deps.blockInfoProvider.fetchCurrentHash()

        guard let collectionId = requestedCollection(for: ring) else {
            throw CreateProofError.unknown("No ring keys configured")
        }

        let selection = try await selectMemberKey(
            in: collectionId,
            deps: deps,
            blockHash: blockHash
        )

        guard let params = try await deps.proofParamsFetcher.fetch(
            for: selection.ringIndex,
            collectionId: collectionId,
            blockHash: blockHash
        ) else {
            throw CreateProofError.ringNotFound
        }

        guard let revision = try await deps.proofParamsFetcher.fetchCurrentRevision(
            for: selection.ringIndex,
            collectionId: collectionId,
            blockHash: blockHash
        ) else {
            throw CreateProofError.ringNotFound
        }

        let proof = try selection.option.keyManager.createProof(
            message,
            members: params.ringMembers,
            context: contextBytes,
            domainSize: params.ringSize
        )

        let alias = try selection.option.keyManager.deriveAlias(for: contextBytes)

        return RingVrfProof(
            proof: proof,
            contextualAlias: ContextualAlias(context: contextBytes, alias: alias),
            ringIndex: selection.ringIndex,
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

    func selectMemberKey(
        in collectionId: MembersPallet.CollectionIdentifier,
        deps: MembershipDeps,
        blockHash: BlockHashData?
    ) async throws -> (option: CreateProofOrAliasOption, ringIndex: MembersPallet.RingIndex) {
        let memberKeys = try options.map { try $0.keyManager.getMemberKey() }

        let statuses = try await deps.statusChecker.checkStatuses(
            of: memberKeys.map { MembershipStatusInput(memberKey: $0, collection: collectionId) },
            blockHash: blockHash
        )

        for (option, memberKey) in zip(options, memberKeys) {
            if let ringIndex = statuses[memberKey] {
                return (option, ringIndex)
            }
        }

        throw CreateProofError.notMember
    }
}
