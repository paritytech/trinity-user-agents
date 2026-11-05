import Foundation
import Keystore_iOS

extension SettingsManagerProtocol {
    /// Whether a provider still worked on a funding session when the app last
    /// heard from the core. The next launch starts the core at once when it
    /// did, so the providers carry on from their last checkpoint instead of
    /// waiting for a screen or product to need the core.
    var isFundingInProgress: Bool {
        value(for: .fundingInProgress)
    }

    func setFundingInProgress(_ inProgress: Bool) {
        set(value: inProgress, for: .fundingInProgress)
    }
}
