import Foundation
import Individuality
import Products
import SubstrateSdk

/// Shared RFC-0004 Accounts Protocol plumbing: ring location resolution and
/// member-key bookkeeping. Operation-specific logic lives in subclasses; all of
/// them must select member keys identically so derived aliases match.
class APBaseMembershipHandler: @unchecked Sendable {
    let callingProductId: ProductId?
    let options: [CreateProofOrAliasOption]
    let logger: LoggerProtocol

    private let depsResolver: MembershipDepsResolving

    init(
        callingProductId: ProductId?,
        options: [CreateProofOrAliasOption],
        depsResolver: MembershipDepsResolving,
        logger: LoggerProtocol
    ) {
        self.callingProductId = callingProductId
        self.options = options
        self.depsResolver = depsResolver
        self.logger = logger
    }

    // Base resolves the ring to raw values; each handler owns its error vocabulary
    // (`CreateProofError` vs `GetAliasError`) and throws at these call sites.

    func resolveDeps(for ring: RingLocation) async throws -> MembershipDeps? {
        try await depsResolver.resolve(
            genesisHash: ring.chainId.toHex(),
            palletIndex: ring.palletInstance
        )
    }

    func isValidRing(_ ring: RingLocation) async throws -> Bool {
        try await depsResolver.validate(
            genesisHash: ring.chainId.toHex(),
            palletIndex: ring.palletInstance
        )
    }

    // No collection junction: fall back to the "PoP" (full-personhood) ring collection.
    func requestedCollection(for ring: RingLocation) -> MembersPallet.CollectionIdentifier? {
        ring.collectionId ?? options.first?.collectionId
    }
}
