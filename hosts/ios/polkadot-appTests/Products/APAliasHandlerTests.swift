import Foundation
import Products
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("APAliasHandler Tests")
struct APAliasHandlerTests {
    @Test("Invalid ring location throws ringNotFound")
    func invalidRingLocationThrowsRingNotFound() async throws {
        let env = APPersonhoodTestEnvironment()
        env.depsResolver.isValidRing = false

        await #expect(throws: GetAliasError.ringNotFound) {
            try await env.makeAliasHandler().getContextualAlias(
                context: env.sameProductContext,
                ring: env.ring
            )
        }
    }

    @Test("Ring location is validated without resolving membership deps")
    func validatesWithoutResolvingDeps() async throws {
        let env = APPersonhoodTestEnvironment()

        let ring = RingLocation(chainId: env.genesisHash, junctions: [.palletInstance(42)])
        _ = try await env.makeAliasHandler().getContextualAlias(
            context: env.sameProductContext,
            ring: ring
        )

        #expect(env.depsResolver.recordedValidationGenesisHash == env.genesisHash.toHex())
        #expect(env.depsResolver.recordedValidationPalletIndex == 42)
        #expect(env.depsResolver.recordedGenesisHash == nil)
    }

    @Test("Alias derivation succeeds without ring membership")
    func aliasIsDeriveOnly() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [:]

        let alias = try await env.makeAliasHandler().getContextualAlias(
            context: env.sameProductContext,
            ring: env.ring
        )

        #expect(alias.alias == env.fullKeyManager.alias)
        #expect(try alias.context == (env.sameProductContext.contextBytes()))
    }

    @Test("Alias key follows the collection junction")
    func aliasKeyFollowsCollectionJunction() async throws {
        let env = APPersonhoodTestEnvironment()

        let ring = RingLocation(
            chainId: env.genesisHash,
            junctions: [.collectionId(env.liteCollectionId)]
        )
        let alias = try await env.makeAliasHandler().getContextualAlias(
            context: env.sameProductContext,
            ring: ring
        )

        #expect(alias.alias == env.liteKeyManager.alias)
    }

    @Test("Denied account access throws rejected")
    func deniedAccountAccessThrowsRejected() async throws {
        let env = APPersonhoodTestEnvironment()
        env.accountAccess.granted = false

        await #expect(throws: GetAliasError.rejected) {
            try await env.makeAliasHandler().getContextualAlias(
                context: env.crossProductContext,
                ring: env.ring
            )
        }

        #expect(env.accountAccess.recordedProductIds == ["caller.product"])
    }
}
