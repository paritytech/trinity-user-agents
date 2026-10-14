package io.paritytech.polkadotapp.feature_people_api.domain

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.toLittleEndianBytes
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

private const val STATEMENT_STORE_SLOT_FAMILY = 2u
private const val LONG_TERM_STORAGE_FAMILY = 3u
private const val PGAS_CLAIM_FAMILY = 4u

class PersonhoodProductContextTest {
    private val tld = requireNotNull(DotNsTld.parse("paseo"))

    @Test
    fun `statement store slot context matches pallet vector`() {
        val context = personhoodProductContext(tld, statementStoreSlotSuffix(period = 100u, seq = 3u))

        assertEquals("b6c21225dcf4c2aeeca32b6db1fc93b6942ca0e8ff5c3cb1b2c5d8f0b4647ee3", context.value.toHexString())
    }

    @Test
    fun `long term storage context matches pallet vector`() {
        val suffix = personhoodSystemSuffix(LONG_TERM_STORAGE_FAMILY, 100u, byteArrayOf(3))
        val context = personhoodProductContext(tld, suffix)

        assertEquals("1b3fbe4dd813ea1e349878c9228c6823db8345207690ca4df656acb7fee81bd1", context.value.toHexString())
    }

    @Test
    fun `pgas claim context matches pallet vector`() {
        val suffix = personhoodSystemSuffix(PGAS_CLAIM_FAMILY, 100u, 3u.toLittleEndianBytes())
        val context = personhoodProductContext(tld, suffix)

        assertEquals("e47ba2c7eae3b97beabaeef8df599afd53e44ba9c2b851cd80850d3ed95a685b", context.value.toHexString())
    }

    @Test
    fun `zero values produce distinct context`() {
        val context = personhoodProductContext(tld, statementStoreSlotSuffix(period = 0u, seq = 0u))

        assertEquals("deee1c90cf0d31093d318ac6629b4c4ab08650d4a4164511cc2496205f20f067", context.value.toHexString())
    }

    @Test
    fun `system suffix layout is pinned`() {
        assertEquals(
            "7379732f02000000640000000300000000000000000000000000000000000000",
            statementStoreSlotSuffix(period = 100u, seq = 3u).bytes.value.toHexString()
        )
        assertEquals(
            "7379732f03000000640000000300000000000000000000000000000000000000",
            personhoodSystemSuffix(LONG_TERM_STORAGE_FAMILY, 100u, byteArrayOf(3)).bytes.value.toHexString()
        )
        assertEquals(
            "7379732f04000000640000000300000000000000000000000000000000000000",
            personhoodSystemSuffix(PGAS_CLAIM_FAMILY, 100u, 3u.toLittleEndianBytes()).bytes.value.toHexString()
        )
        assertEquals(
            "7379732f02000000000000000000000000000000000000000000000000000000",
            statementStoreSlotSuffix(period = 0u, seq = 0u).bytes.value.toHexString()
        )
    }

    @Test
    fun `index suffix layout is pinned`() {
        val indexMagic = "12e86013736c5498f050b03cdc16957dff0e422fb92ca77ec3ab168f"

        assertEquals("00000000" + indexMagic, DerivationIndex32.fromUInt(0u).bytes.value.toHexString())
        assertEquals("01000000" + indexMagic, DerivationIndex32.fromUInt(1u).bytes.value.toHexString())
    }

    @Test
    fun `different network suffix changes context`() {
        val suffix = statementStoreSlotSuffix(period = 100u, seq = 3u)

        assertNotEquals(
            personhoodProductContext(requireNotNull(DotNsTld.parse("paseo")), suffix).value.toHexString(),
            personhoodProductContext(requireNotNull(DotNsTld.parse("dot")), suffix).value.toHexString()
        )
    }

    private fun statementStoreSlotSuffix(period: UInt, seq: UInt): DerivationIndex32 {
        return personhoodSystemSuffix(STATEMENT_STORE_SLOT_FAMILY, period, seq.toLittleEndianBytes())
    }
}
