import Foundation

public extension AssetModel {
    var supportsNominationPoolsStaking: Bool {
        guard let stakings else {
            return false
        }

        return stakings.contains(.nominationPools)
    }

    var hasMultipleStakingOptions: Bool {
        (stakings ?? []).count > 1
    }
}
