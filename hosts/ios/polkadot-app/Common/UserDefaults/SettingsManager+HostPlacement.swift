import Foundation
import Keystore_iOS

extension SettingsManagerProtocol {
    /// Whether the host puts its designated products in chat itself.
    ///
    /// A stored choice wins. The default cannot live in `value(for:)`, which is shared by
    /// every boolean setting.
    var isHostPlacementEnabled: Bool {
        bool(for: SettingsKey.hostPlacementEnabled.rawValue) ?? true
    }
}
