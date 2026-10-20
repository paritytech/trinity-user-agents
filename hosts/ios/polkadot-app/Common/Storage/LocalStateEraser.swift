import Foundation
import Keystore_iOS

protocol LocalStateErasing {
    func eraseUserState()
}

/// Erases the UserDefaults values that belong to a wallet rather than to the device, and the auth
/// tokens products kept for it, so a wallet this device cannot use (its entropy never left the device
/// it was created on) leaves no identity, progress or credential behind for the wallet onboarded
/// next. Preferences, notification bookkeeping and the ids that index self-healing Keychain items
/// stay; databases and the rest of the Keychain are not touched.
final class LocalStateEraser: LocalStateErasing {
    /// Identity the launch gates and claims read: the username gate would otherwise pass a new wallet
    /// under the old name.
    static let identityKeys: [SettingsKey] = [.username, .usernameClaimed]

    /// Progress bound to the old wallet that would misreport for a new one.
    static let walletProgressKeys: [SettingsKey] = [
        .backendSessionId,
        .nextSyncUpdateId,
        .fiatOnrampSessionIds,
        .fiatOnrampTrackedTransactionIds
    ]

    private let settingsManager: SettingsManagerProtocol
    private let authTokens: AuthTokenKeychain
    private let logger: LoggerProtocol

    init(
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        authTokens: AuthTokenKeychain = SecAuthTokenKeychain(),
        logger: LoggerProtocol
    ) {
        self.settingsManager = settingsManager
        self.authTokens = authTokens
        self.logger = logger
    }

    func eraseUserState() {
        for key in Self.identityKeys + Self.walletProgressKeys {
            settingsManager.removeValue(for: key)
        }
        do {
            try authTokens.deleteAll(productId: nil)
        } catch {
            logger.error("Products' auth tokens were not erased: \(error)")
        }
        logger.info("Erased the previous wallet's identity, progress settings and auth tokens")
    }
}
