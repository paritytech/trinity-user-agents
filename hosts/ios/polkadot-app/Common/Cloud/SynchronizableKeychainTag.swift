import Foundation

enum SynchronizableKeychainTag {
    static let domain = "io.polkadotapp.cloud.keychain"

    static var walletEntropy: String {
        domain + "." + "wallet.entropy.v2"
    }
}
