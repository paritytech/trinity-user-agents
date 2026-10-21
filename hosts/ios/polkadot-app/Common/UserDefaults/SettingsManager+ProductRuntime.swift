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

    /// Whether the chat product, rather than the native chat, runs the user's chats (HX2). Debug and Nightly start
    /// on the chat product. Read through ``PersonChatMode``, which fixes it for the launch.
    var isProductChatEnabled: Bool {
        bool(for: SettingsKey.productChatEnabled.rawValue) ?? defaultTrUAPIRuntimeEnabled
    }
}

/// Which chat runs the user's chats on this install (HX2): the native chat or the chat product, never both, since
/// both would answer the same statements and sessions. Fixed at launch; a change takes effect at the next one.
enum PersonChatMode {
    static let isProductChat = BundledChatProduct.coreRuntimeAvailable && SettingsManager.shared.isProductChatEnabled
}
