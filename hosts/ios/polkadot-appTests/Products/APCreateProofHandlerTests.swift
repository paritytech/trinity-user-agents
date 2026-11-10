import Foundation
import Products
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("APCreateProofHandler Tests")
struct APCreateProofHandlerTests {
    @Test("Unresolved membership deps throw ringNotFound")
    func unresolvedDepsThrowRingNotFound() async throws {
        let env = APPersonhoodTestEnvironment()
        env.depsResolver.deps = nil

        await #expect(throws: CreateProofError.ringNotFound) {
            try await createProof(env)
        }
    }

    @Test("Genesis hash and pallet index junction are forwarded to the resolver")
    func ringLocationForwardedToResolver() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [env.keyManager.publicKey: 5]

        let ring = RingLocation(
            chainId: env.genesisHash,
            junctions: [.palletInstance(42), .collectionId(env.liteCollectionId)]
        )
        _ = try await createProof(env, ring: ring)

        #expect(env.depsResolver.recordedGenesisHash == env.genesisHash.toHex())
        #expect(env.depsResolver.recordedPalletIndex == 42)
    }

    @Test("The caller's key handle selects the key")
    func keyHandleIsForwarded() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [env.keyManager.publicKey: 5]

        _ = try await createProof(env)

        #expect(env.builtInKeys.recordedHandles == [env.liteHandle])
    }

    @Test("Only the handle's key is checked for membership")
    func onlyHandleKeyIsChecked() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [MockBandersnatchKeyManager().publicKey: 5]

        await #expect(throws: CreateProofError.notMember) {
            try await createProof(env)
        }
    }

    @Test(
        "Key resolution errors map to the proof error",
        arguments: [
            (BuiltInPersonhoodKeyError.keyNotRegistered, CreateProofError.keyNotRegistered),
            (BuiltInPersonhoodKeyError.keyNotInRing, CreateProofError.keyNotInRing)
        ]
    )
    func keyResolutionErrors(source: BuiltInPersonhoodKeyError, expected: CreateProofError) async {
        let env = APPersonhoodTestEnvironment()
        env.builtInKeys.resolveError = source

        await #expect(throws: expected) {
            try await createProof(env)
        }
        #expect(env.depsResolver.recordedGenesisHash == nil)
    }

    @Test("Cross-product rejection throws rejected before any key or chain work")
    func crossProductRejectionThrowsRejected() async throws {
        let env = APPersonhoodTestEnvironment()
        env.confirmation.decision = .rejected

        await #expect(throws: CreateProofError.rejected) {
            try await createProof(env, context: env.crossProductContext)
        }

        #expect(env.builtInKeys.recordedHandles.isEmpty)
        #expect(env.depsResolver.recordedGenesisHash == nil)
    }

    @Test("Approved cross-product proof succeeds after one confirmation")
    func crossProductApprovalSucceeds() async throws {
        let env = APPersonhoodTestEnvironment()
        env.confirmation.decision = .approved
        env.statusChecker.statuses = [env.keyManager.publicKey: 5]

        let proof = try await createProof(env, context: env.crossProductContext)

        #expect(env.confirmation.requests.count == 1)
        #expect(proof.proof == env.keyManager.proof)
    }

    @Test("Same-product proof succeeds without confirmation")
    func sameProductProofSucceeds() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [env.keyManager.publicKey: 5]

        let proof = try await createProof(env)

        #expect(env.confirmation.requests.isEmpty)
        #expect(proof.proof == env.keyManager.proof)
        #expect(proof.contextualAlias.alias == env.keyManager.alias)
        #expect(try proof.contextualAlias.context == (env.sameProductContext.contextBytes()))
        #expect(proof.ringIndex == 5)
        #expect(proof.ringRevision == env.paramsFetcher.revision)
    }

    @Test("All reads are pinned to the same current block hash")
    func readsArePinnedToOneBlockHash() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [env.keyManager.publicKey: 5]

        _ = try await createProof(env)

        #expect(env.statusChecker.recordedBlockHash == env.blockInfoProvider.currentHash)
        #expect(env.paramsFetcher.recordedFetchBlockHash == env.blockInfoProvider.currentHash)
        #expect(env.paramsFetcher.recordedRevisionBlockHash == env.blockInfoProvider.currentHash)
    }
}

private extension APCreateProofHandlerTests {
    func createProof(
        _ env: APPersonhoodTestEnvironment,
        context: ProductProofContext? = nil,
        ring: RingLocation? = nil
    ) async throws -> RingVrfProof {
        try await env.makeCreateProofHandler().createProof(
            keyHandle: env.liteHandle,
            context: context ?? env.sameProductContext,
            ring: ring ?? env.ring,
            message: env.message
        )
    }
}
