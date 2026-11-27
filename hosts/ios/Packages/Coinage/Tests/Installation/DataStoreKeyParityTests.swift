import Foundation
import NovaCrypto
import SubstrateSdk
import Testing
@testable import Coinage

/// Pinned against Android's `DataStoreKeyParityTest` (itself derived from @polkadot/util-crypto's
/// `mnemonicToMiniSecret` → `sr25519PairFromSeed` → `keyFromPath`). A drift here would make records the other
/// platform wrote decrypt to nothing and be skipped silently. The secret is the canonical sr25519 scalar
/// followed by the nonce; polkadot-js reports the scalar × 8 and that form is not used.
struct DataStoreKeyParityTests {
    private static let mnemonic = "bottom drive obey lake curtain smoke basket hold race lonely fit walk"

    private static let dataStorePublicKey = "0x82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d"
    private static let dataStoreSecret = "0x1f11025a1b4ac9f03e0318998b35c52db71a7abcbc83aa9627becbd21e012909"
        + "2dddcd798787957c4b5d3b960b4b78d0c925adeb662079c192c01ba13c4f9abe"
    private static let encryptionKey = "0xf7453a63782c0b1469c928ed8579a3c0dc32c9a9cc3449fe4ce85ed79b2777b6"

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
        #expect(account.privateKey.toHex(includePrefix: true) == Self.dataStoreSecret)
        #expect(account.encryptionKey.toHex(includePrefix: true) == Self.encryptionKey)
    }

    @Test("an installation-scoped coin path matches polkadot-js and Android")
    func coinPath() throws {
        let factory = CoinKeypairFactory(entropyManager: entropyManager)
        let index = CoinageKeyIndex(installation: .fixed(0x5A), item: 7)

        #expect(try factory.derivePublicKey(index: index).toHex(includePrefix: true) == Self.coinPublicKey)
    }
}
