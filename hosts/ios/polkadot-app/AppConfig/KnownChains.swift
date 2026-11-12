import Foundation
import SubstrateSdk

enum KnownChainId {
    #if UNSTABLE
        static let previewNetPeople = "preview-people"
        static let previewNetBulletIn = "preview-bulletin"
        static let polkadotAH = "68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f"
        static let polkadotPeople = "67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008"
        static let hydration = "afdc188f45c71dacbaa0b62e16a91f726c7b8699a9748cdf715459de6b7f366d"
        static let paseoBulletIn = "paseo-bulletin"
        static let paseoAH = "paseo-asset-hub"
        static let previewAH = "preview-ah"
    #elseif NIGHTLY || SAFETYNET
        static let paseoRelay = "nightly-relay"
        static let paseoAH = "nightly-ah"
        static let paseoPeople = "nightly-people"
        static let paseoBulletIn = "nightly-bulletin"
    #else
        static let releaseRelay = "release-relay"
        static let releaseAH = "release-ah"
        static let releasePeople = "release-people"
        static let releaseBulletIn = "release-bulletin"
    #endif
}
