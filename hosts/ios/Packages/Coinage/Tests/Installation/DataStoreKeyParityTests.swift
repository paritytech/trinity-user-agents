import Foundation
import NovaCrypto
import SubstrateSdk
import Testing
@testable import Coinage

/// Pinned against @polkadot/util-crypto (`mnemonicToMiniSecret` → `sr25519PairFromSeed` → `keyFromPath`),
/// the same vectors Android's `DataStoreKeyParityTest` pins. A drift here would make records the other
/// platform wrote decrypt to nothing and be skipped silently. polkadot-js reports the secret in schnorrkel's
/// ed25519-expanded form; the iOS SDK holds the canonical scalar, so the secret is compared after conversion.
struct DataStoreKeyParityTests {
    private static let mnemonic = "bottom drive obey lake curtain smoke basket hold race lonely fit walk"

    private static let dataStorePublicKey = "0x82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d"
    private static let dataStoreSecret = "0xf88810d0da504a86f719c0c85cac296eb9d5d0e3e51d54b53cf15d96f6084849"
        + "2dddcd798787957c4b5d3b960b4b78d0c925adeb662079c192c01ba13c4f9abe"
    private static let encryptionKey = "0xa217d0089ea2bc908772a8df8ab8f4aeb122eb1f41bbd26b6aff9c9386143f2a"

    private static let coinPublicKey = "0x4eced11d18ac64e54559cc45eb79415ca66d9354e74394dc8729b69a0fbd9107"

    private let entropyManager: MockEntropyManager

    init() throws {
        let mnemonic = try IRMnemonicCreator().mnemonic(fromList: Self.mnemonic)
        entropyManager = MockEntropyManager(entropy: mnemonic.entropy())
    }

    @Test("the data store keypair and its encryption key match polkadot-js and Android")
    func dataStoreKeys() async throws {
        let account = try await DataStoreAccountKeys(entropyManager: entropyManager).account()

        #expect(account.publicKey.toHex(includePrefix: true) == Self.dataStorePublicKey)
        let secret64 = try DataStoreAccountKeys.ed25519Form(of: account.privateKey)
        #expect(secret64.toHex(includePrefix: true) == Self.dataStoreSecret)
        #expect(account.encryptionKey.toHex(includePrefix: true) == Self.encryptionKey)
    }

    @Test("an installation-scoped coin path matches polkadot-js and Android")
    func coinPath() throws {
        let factory = CoinKeypairFactory(entropyManager: entropyManager)
        let index = CoinageKeyIndex(installation: .fixed(0x5A), item: 7)

        #expect(try factory.derivePublicKey(index: index).toHex(includePrefix: true) == Self.coinPublicKey)
    }
}
