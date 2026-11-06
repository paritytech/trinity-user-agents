import struct TrUAPIHost.ProductId
import Foundation
import Individuality
import Products
import SubstrateSdk

@testable import polkadot_app

final class APPersonhoodTestEnvironment {
    let genesisHash = Data.random(of: 32)!
    let message = Data.random(of: 64)!
    let liteCollectionId = PeopleLitePallet.membersIdentifier

    let liteHandle = RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .index(1))
    let sameProductContext = ProductProofContext(productId: try! ProductId("caller.product.dot"), suffix: .index(0))
    let crossProductContext = ProductProofContext(productId: try! ProductId("other.product.dot"), suffix: .index(0))

    let keyManager = MockBandersnatchKeyManager()
    let builtInKeys = MockBuiltInPersonhoodKeys()
    let statusChecker = MockMembershipStatusChecker()
    let paramsFetcher = MockMembershipProofParamsFetcher()
    let blockInfoProvider = MockBlockInfoProvider()
    let depsResolver = MockMembershipDepsResolver()
    let confirmation = MockCreateProofConfirmation()
    let accountAccess = MockAccountAccessPermissionHandler()

    var ring: RingLocation {
        RingLocation(chainId: genesisHash, junctions: [.collectionId(liteCollectionId)])
    }

    init() {
        builtInKeys.resolvedKeyManager = keyManager
        depsResolver.deps = MembershipDeps(
            statusChecker: statusChecker,
            proofParamsFetcher: paramsFetcher,
            blockInfoProvider: blockInfoProvider
        )
    }

    func makeCreateProofHandler(callingProductId: ProductId? = try! ProductId("caller.product.dot"))
        -> APCreateProofHandler {
        APCreateProofHandler(
            callingProductId: callingProductId,
            builtInKeys: builtInKeys,
            depsResolver: depsResolver,
            confirmationRequester: confirmation,
            logger: Logger.shared
        )
    }

    func makeAliasHandler(callingProductId: ProductId? = try! ProductId("caller.product.dot")) -> APAliasHandler {
        APAliasHandler(
            callingProductId: callingProductId,
            builtInKeys: builtInKeys,
            depsResolver: depsResolver,
            accountAccessHandler: accountAccess,
            logger: Logger.shared
        )
    }
}
