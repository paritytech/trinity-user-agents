import SubstrateSdk
import Foundation

public extension ChainModel {
    var stakingWiki: URL? {
        guard let wiki = additional?.stakingWiki?.stringValue else {
            return nil
        }

        return URL(string: wiki)
    }
}
