import Foundation
import SubstrateSdk
import Foundation_iOS

/// Describes which connections a retain applies to.
public enum ConnectionRetainScope {
    /// Retain the current connections for the given chains. Chains without a live
    /// connection are skipped.
    case chains([ChainModel.Id])

    /// Retain every connection and keep the pool awake across backgrounding for as long
    /// as the token is held — including connections created while the retain is active.
    case all
}
