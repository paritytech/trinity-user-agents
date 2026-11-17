import struct TrUAPIHost.ProductId
import Foundation
import Individuality
import KeyDerivation
import Products
import SubstrateSdk

class APBaseMembershipHandler: @unchecked Sendable {
    let callingProductId: ProductId?
    let builtInKeys: BuiltInPersonhoodKeysProviding
    let logger: LoggerProtocol

    private let depsResolver: MembershipDepsResolving

    init(
        callingProductId: ProductId?,
        builtInKeys: BuiltInPersonhoodKeysProviding,
        depsResolver: MembershipDepsResolving,
        logger: LoggerProtocol
    ) {
        self.callingProductId = callingProductId
        self.builtInKeys = builtInKeys
        self.depsResolver = depsResolver
        self.logger = logger
    }

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
}
