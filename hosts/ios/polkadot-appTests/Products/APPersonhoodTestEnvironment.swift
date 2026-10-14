import Foundation
import Individuality
import Products
import SubstrateSdk

@testable import polkadot_app

/// Shared fixture for Accounts Protocol handler tests: mocked seams wired
/// around resolvable membership deps.
final class APPersonhoodTestEnvironment {
    let genesisHash = Data.random(of: 32)!
    let message = Data.random(of: 64)!
    let fullCollectionId = PeoplePallet.membersIdentifier
    let liteCollectionId = PeopleLitePallet.membersIdentifier

    let sameProductContext = ProductProofContext(productId: "caller.product", suffix: .index(0))
    let crossProductContext = ProductProofContext(productId: "other.product", suffix: .index(0))

    let fullKeyManager = MockBandersnatchKeyManager()
    let liteKeyManager = MockBandersnatchKeyManager()
    let statusChecker = MockMembershipStatusChecker()
    let paramsFetcher = MockMembershipProofParamsFetcher()
    let blockInfoProvider = MockBlockInfoProvider()
    let depsResolver = MockMembershipDepsResolver()
    let confirmation = MockCreateProofConfirmation()
    let accountAccess = MockAccountAccessPermissionHandler()

    var ring: RingLocation {
        RingLocation(chainId: genesisHash, junctions: [])
    }

    init() {
        depsResolver.deps = MembershipDeps(
            statusChecker: statusChecker,
            proofParamsFetcher: paramsFetcher,
            blockInfoProvider: blockInfoProvider
        )
    }

    func makeCreateProofHandler(callingProductId: ProductId? = "caller.product") -> APCreateProofHandler {
        APCreateProofHandler(
            callingProductId: callingProductId,
            options: makeOptions(),
            depsResolver: depsResolver,
            confirmationRequester: confirmation,
            logger: Logger.shared
        )
    }

    func makeAliasHandler(callingProductId: ProductId? = "caller.product") -> APAliasHandler {
        APAliasHandler(
            callingProductId: callingProductId,
            options: makeOptions(),
            depsResolver: depsResolver,
            accountAccessHandler: accountAccess,
            logger: Logger.shared
        )
    }
}

private extension APPersonhoodTestEnvironment {
    func makeOptions() -> [CreateProofOrAliasOption] {
        [
            CreateProofOrAliasOption(collectionId: fullCollectionId, keyManager: fullKeyManager),
            CreateProofOrAliasOption(collectionId: liteCollectionId, keyManager: liteKeyManager)
        ]
    }
}
