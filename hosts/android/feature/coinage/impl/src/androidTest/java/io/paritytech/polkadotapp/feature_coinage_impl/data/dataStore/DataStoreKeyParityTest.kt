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
// has to agree with. polkadot-js returns the secret with the scalar multiplied by 8; DATA_STORE_SECRET is that scalar
// divided by 8, followed by the nonce. sr25519 needs its native library, so this runs on a device rather than the JVM.
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
        const val DATA_STORE_SECRET = "0x1f11025a1b4ac9f03e0318998b35c52db71a7abcbc83aa9627becbd21e012909" +
            "2dddcd798787957c4b5d3b960b4b78d0c925adeb662079c192c01ba13c4f9abe"
        const val ENCRYPTION_KEY = "0xf7453a63782c0b1469c928ed8579a3c0dc32c9a9cc3449fe4ce85ed79b2777b6"

        const val COIN_PATH = "//coinage//4294967295//0x5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a/7"
        const val COIN_PUBLIC_KEY = "0x4eced11d18ac64e54559cc45eb79415ca66d9354e74394dc8729b69a0fbd9107"
    }
}
