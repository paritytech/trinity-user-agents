import Foundation
import Keystore_iOS

#if TRUAPI_RUNTIME_DEFAULT
    private let defaultTrUAPIRuntimeEnabled = true
#else
    private let defaultTrUAPIRuntimeEnabled = false
#endif

extension SettingsManagerProtocol {
    /// Debug and Nightly set `TRUAPI_RUNTIME_DEFAULT` (see Configs/base.debug.xcconfig and
    /// Configs/base.nightly.xcconfig) so developers and testers land on TrUAPI without opening
    /// Debug Settings; every other configuration defaults to native.
    /// The default cannot live in `value(for:)` — that helper is shared by every boolean setting.
    var isTrUAPIRuntimeEnabled: Bool {
        bool(for: SettingsKey.truApiRuntimeEnabled.rawValue) ?? defaultTrUAPIRuntimeEnabled
    }
}
