package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import org.junit.Assert.assertEquals
import org.junit.Test

class SelectedVoucherKeysCodecTest {
    private val codec = SelectedVoucherKeysCodec()

    @Test
    fun `keys from several installations survive a round trip in order`() {
        val keys = listOf(
            CoinageKeyIndex(TEST_INSTALLATION, 7),
            CoinageKeyIndex(OTHER_INSTALLATION, 0),
            CoinageKeyIndex(INSTALLATION_5A, 1_234_567),
        )

        assertEquals(keys, codec.decode(codec.encode(keys)))
    }

    @Test
    fun `no keys survive a round trip`() {
        assertEquals(emptyList<CoinageKeyIndex>(), codec.decode(codec.encode(emptyList())))
    }

    @Test
    fun `keys are persisted as a scale vector of installation and item`() {
        val encoded = codec.encode(listOf(CoinageKeyIndex(INSTALLATION_5A, 7)))

        assertEquals("0x04" + "80" + "5a".repeat(32) + "07000000", encoded)
    }

    private companion object {
        val OTHER_INSTALLATION = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x11 }.toDataByteArray())
        val INSTALLATION_5A = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x5a }.toDataByteArray())
    }
}
