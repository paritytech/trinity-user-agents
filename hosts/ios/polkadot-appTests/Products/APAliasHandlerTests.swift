import struct TrUAPIHost.ProductId
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
            try await getAlias(env)
        }
    }

    @Test("Ring location is validated without resolving membership deps")
    func validatesWithoutResolvingDeps() async throws {
        let env = APPersonhoodTestEnvironment()

        let ring = RingLocation(
            chainId: env.genesisHash,
            junctions: [.palletInstance(42), .collectionId(env.liteCollectionId)]
        )
        _ = try await getAlias(env, ring: ring)

        #expect(env.depsResolver.recordedValidationGenesisHash == env.genesisHash.toHex())
        #expect(env.depsResolver.recordedValidationPalletIndex == 42)
        #expect(env.depsResolver.recordedGenesisHash == nil)
    }

    @Test("Alias derivation uses the handle's key without ring membership")
    func aliasIsDeriveOnly() async throws {
        let env = APPersonhoodTestEnvironment()
        env.statusChecker.statuses = [:]

        let alias = try await getAlias(env)

        #expect(env.builtInKeys.recordedHandles == [env.liteHandle])
        #expect(alias.alias == env.keyManager.alias)
        #expect(try alias.context == (env.sameProductContext.contextBytes()))
    }

    @Test(
        "Key resolution errors map to the alias error",
        arguments: [
            (BuiltInPersonhoodKeyError.keyNotRegistered, GetAliasError.keyNotRegistered),
            (BuiltInPersonhoodKeyError.keyNotInRing, GetAliasError.keyNotInRing)
        ]
    )
    func keyResolutionErrors(source: BuiltInPersonhoodKeyError, expected: GetAliasError) async {
        let env = APPersonhoodTestEnvironment()
        env.builtInKeys.resolveError = source

        await #expect(throws: expected) {
            try await getAlias(env)
        }
    }

    @Test("Denied account access throws rejected")
    func deniedAccountAccessThrowsRejected() async throws {
        let env = APPersonhoodTestEnvironment()
        env.accountAccess.granted = false

        await #expect(throws: GetAliasError.rejected) {
            try await getAlias(env, context: env.crossProductContext)
        }

        #expect(env.accountAccess.recordedProductIds == [try! ProductId("caller.product.dot")])
    }
}

private extension APAliasHandlerTests {
    func getAlias(
        _ env: APPersonhoodTestEnvironment,
        context: ProductProofContext? = nil,
        ring: RingLocation? = nil
    ) async throws -> ContextualAlias {
        try await env.makeAliasHandler().getContextualAlias(
            keyHandle: env.liteHandle,
            context: context ?? env.sameProductContext,
            ring: ring ?? env.ring
        )
    }
}
