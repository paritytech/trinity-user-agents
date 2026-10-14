import Coinage
import Foundation
import KeyDerivation

/// Scopes the coinage allocation counters by the installation they count for, in the app's Keychain
/// tag layout.
struct CoinageInstallationKeychainTags: CoinageInstallationKeychainTagProviding {
    func coinIndexTag(for installation: CoinageInstallationId) -> String {
        KeystoreTag.coinageCoinIndexTag(forInstallation: installation.hex)
    }

    func voucherIndexTag(for installation: CoinageInstallationId) -> String {
        KeystoreTag.coinageVoucherIndexTag(forInstallation: installation.hex)
    }
}
