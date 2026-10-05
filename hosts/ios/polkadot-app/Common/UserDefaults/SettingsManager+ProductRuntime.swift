import Foundation
import Keystore_iOS

#if TRUAPI_RUNTIME_DEFAULT
    private let defaultTrUAPIRuntimeEnabled = true
#else
    private let defaultTrUAPIRuntimeEnabled = false
#endif

private let selectedTrUAPIRuntime = SettingsManager.shared.bool(for: SettingsKey.truApiRuntimeEnabled.rawValue) ?? defaultTrUAPIRuntimeEnabled

extension SettingsManagerProtocol {
    /// Nightly sets `TRUAPI_RUNTIME_DEFAULT` (see Configs/base.nightly.xcconfig) so testers land on
    /// TrUAPI without opening Debug Settings; every other configuration defaults to native.
    /// The default cannot live in `value(for:)` — that helper is shared by every boolean setting.
    var isTrUAPIRuntimeEnabled: Bool {
        if self as AnyObject === SettingsManager.shared {
            return selectedTrUAPIRuntime
        }
        return configuredTrUAPIRuntimeEnabled
    }
    var configuredTrUAPIRuntimeEnabled: Bool {
        bool(for: SettingsKey.truApiRuntimeEnabled.rawValue) ?? defaultTrUAPIRuntimeEnabled
    }
}
