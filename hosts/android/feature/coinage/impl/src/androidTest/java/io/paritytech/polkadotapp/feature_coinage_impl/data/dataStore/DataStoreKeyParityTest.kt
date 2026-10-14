package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import androidx.test.ext.junit.runners.AndroidJUnit4
import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.novasama.substrate_sdk_android.encrypt.junction.decode
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.seed.substrate.SubstrateSeedFactory
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.util.deriveSeed32
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

// Pinned against @polkadot/util-crypto (mnemonicToMiniSecret → sr25519PairFromSeed → keyFromPath), which is what iOS
// has to agree with. sr25519 needs its native library, so this runs on a device rather than the JVM.
@RunWith(AndroidJUnit4::class)
class DataStoreKeyParityTest {
    @Test
    fun theDataStoreKeypairAndItsEncryptionKeyMatchPolkadotJs() {
        val keypair = derive("//datastore")
        val secret64 = keypair.privateKey + keypair.nonce

        assertEquals(DATA_STORE_PUBLIC_KEY, keypair.publicKey.toHexString(withPrefix = true))
        assertEquals(DATA_STORE_SECRET, secret64.toHexString(withPrefix = true))
        assertEquals(ENCRYPTION_KEY, deriveDataStoreEncryptionKey(secret64).bytes.value.toHexString(withPrefix = true))
    }

    @Test
    fun anInstallationScopedCoinPathMatchesPolkadotJs() {
        assertEquals(COIN_PUBLIC_KEY, derive(COIN_PATH).publicKey.toHexString(withPrefix = true))
    }

    private fun derive(path: String): Sr25519Keypair {
        val seed = SubstrateSeedFactory.deriveSeed32(MNEMONIC, password = null).seed
        val junctions = SubstrateJunctionDecoder.decode(path).junctions

        return SubstrateKeypairFactory.generate(EncryptionType.SR25519, seed, junctions) as Sr25519Keypair
    }

    private companion object {
        const val MNEMONIC = "bottom drive obey lake curtain smoke basket hold race lonely fit walk"

        const val DATA_STORE_PUBLIC_KEY = "0x82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d"
        const val DATA_STORE_SECRET = "0xf88810d0da504a86f719c0c85cac296eb9d5d0e3e51d54b53cf15d96f6084849" +
            "2dddcd798787957c4b5d3b960b4b78d0c925adeb662079c192c01ba13c4f9abe"
        const val ENCRYPTION_KEY = "0xa217d0089ea2bc908772a8df8ab8f4aeb122eb1f41bbd26b6aff9c9386143f2a"

        const val COIN_PATH = "//coinage//4294967295//0x5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a/7"
        const val COIN_PUBLIC_KEY = "0x4eced11d18ac64e54559cc45eb79415ca66d9354e74394dc8729b69a0fbd9107"
    }
}
