import Foundation
import Keystore_iOS

enum SettingsKey: String {
    case username = "username.v3"
    case usernameClaimed = "usernameClaimed.v3"
    case isPerson = "isPerson.v3"
    case waitingRoomNotificationDate
    case gameStartNotificationDate
    case registrationStartNotificationDates
    case gameAboutToStartNotificationDate
    case registrationOpenNotificationDates
    case voucherInUseDismissed = "voucherInUseDismissed.v3"
    case playerTooltipShown
    case swipeTooltipShown
    case selectedCurrencyCode
    case fiatOnrampSessionIds
    case fiatOnrampTrackedTransactionIds
    case gameAlarmId
    case gameAlarmFireDate
    case gameAlarmTimingSeconds
    case productGameReminders
    // Balance restored notification
    case deviceEncryptId = "deviceEncryptId.v3"
    // App Group suite. The raw key predates the rename from "entropy id" and is kept so existing
    // installs keep resolving their Keychain items.
    case installationKeyId = "io.polkadot.app.entropy.id.v3"
    case nextSyncUpdateId = "nextSyncUpdateId.v3"
    case themeSelected = "themeSelected.v3"
    case gameCalendarReminder
    case localNetworkPermissionRequested
    case backendSessionId = "backendSessionId.v3"
    case showTransferStrategyDebug
    case truApiRuntimeEnabled
    case hostPlacementEnabled
    case coinageRecyclingStrategy
    case tabBarLabelsEnabled
    case debugPocketCards
    #if TESTNET_FEATURE
        case tipsResetPending
    #endif
}

extension SettingsManagerProtocol {
    func integerArray(for setting: SettingsKey) -> [Int]? {
        anyValue(for: setting.rawValue) as? [Int]
    }

    func removeValue(for setting: SettingsKey) {
        removeValue(for: setting.rawValue)
    }

    func integer(for setting: SettingsKey) -> Int? {
        integer(for: setting.rawValue)
    }

    func set(value: Int, for setting: SettingsKey) {
        set(value: value, for: setting.rawValue)
    }

    func set(intArray: [Int], for setting: SettingsKey) {
        set(anyValue: intArray, for: setting.rawValue)
    }

    func set(value: Bool, for setting: SettingsKey) {
        set(value: value, for: setting.rawValue)
    }

    func value(for setting: SettingsKey) -> Bool {
        bool(for: setting.rawValue) ?? false
    }

    func set(string value: String, for key: SettingsKey) {
        set(string: value, for: key.rawValue)
    }

    func string(for key: SettingsKey) -> String? {
        string(for: key.rawValue)
    }

    func set(string value: String, for key: String) {
        set(anyValue: value, for: key)
    }

    func string(for key: String) -> String? {
        anyValue(for: key) as? String
    }
}
