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
            try await env.makeCreateProofHandler().createProof(
                context: env.sameProductContext,
                ring: env.ring,
                message: env.message
            )
        }
    }

    @Test("Genesis hash and pallet index junction are forwarded to the resolver")
    func ringLocationForwardedToResolver() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [env.fullKeyManager.publicKey: 5]

        let ring = RingLocation(chainId: env.genesisHash, junctions: [.palletInstance(42)])
        _ = try await env.makeCreateProofHandler().createProof(
            context: env.sameProductContext,
            ring: ring,
            message: env.message
        )

        #expect(env.depsResolver.recordedGenesisHash == env.genesisHash.toHex())
        #expect(env.depsResolver.recordedPalletIndex == 42)
    }

    @Test("Missing membership throws notMember")
    func missingMembershipThrowsNotMember() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [:]

        await #expect(throws: CreateProofError.notMember) {
            try await env.makeCreateProofHandler().createProof(
                context: env.sameProductContext,
                ring: env.ring,
                message: env.message
            )
        }
    }

    @Test("Cross-product rejection throws rejected before any chain work")
    func crossProductRejectionThrowsRejected() async throws {
        let env = APPersonhoodTestEnvironment()
        env.confirmation.decision = .rejected

        await #expect(throws: CreateProofError.rejected) {
            try await env.makeCreateProofHandler().createProof(
                context: env.crossProductContext,
                ring: env.ring,
                message: env.message
            )
        }

        #expect(env.depsResolver.recordedGenesisHash == nil)
    }

    @Test("Same-product proof succeeds without confirmation")
    func sameProductProofSucceeds() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [env.fullKeyManager.publicKey: 5]

        let proof = try await env.makeCreateProofHandler().createProof(
            context: env.sameProductContext,
            ring: env.ring,
            message: env.message
        )

        #expect(env.confirmation.requests.isEmpty)
        #expect(proof.proof == env.fullKeyManager.proof)
        #expect(proof.contextualAlias.alias == env.fullKeyManager.alias)
        #expect(try proof.contextualAlias.context == (env.sameProductContext.contextBytes()))
        #expect(proof.ringIndex == 5)
        #expect(proof.ringRevision == env.paramsFetcher.revision)
    }

    @Test("All reads are pinned to the same current block hash")
    func readsArePinnedToOneBlockHash() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [env.fullKeyManager.publicKey: 5]

        _ = try await env.makeCreateProofHandler().createProof(
            context: env.sameProductContext,
            ring: env.ring,
            message: env.message
        )

        #expect(env.statusChecker.recordedBlockHash == env.blockInfoProvider.currentHash)
        #expect(env.paramsFetcher.recordedFetchBlockHash == env.blockInfoProvider.currentHash)
        #expect(env.paramsFetcher.recordedRevisionBlockHash == env.blockInfoProvider.currentHash)
    }

    @Test("Full person key wins when both keys are members")
    func fullPersonKeyHasPriority() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [
            env.fullKeyManager.publicKey: 5,
            env.liteKeyManager.publicKey: 8
        ]

        let proof = try await env.makeCreateProofHandler().createProof(
            context: env.sameProductContext,
            ring: env.ring,
            message: env.message
        )

        #expect(proof.proof == env.fullKeyManager.proof)
        #expect(proof.ringIndex == 5)
    }
}
