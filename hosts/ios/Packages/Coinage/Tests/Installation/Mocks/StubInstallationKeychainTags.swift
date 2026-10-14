import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

/// Tags scoped by the installation, mirroring the app's `KeystoreTag` layout.
struct StubInstallationKeychainTags: CoinageInstallationKeychainTagProviding {
    func coinIndexTag(for installation: CoinageInstallationId) -> String {
        tag("coinage.coin.index", installation)
    }

    func voucherIndexTag(for installation: CoinageInstallationId) -> String {
        tag("coinage.voucher.index", installation)
    }

    private func tag(_ item: String, _ installation: CoinageInstallationId) -> String {
        ["io.polkadotapp", installation.hex, item].joined(separator: ":")
    }
}
