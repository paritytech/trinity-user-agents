@file:OptIn(ExperimentalStdlibApi::class)

package io.paritytech.polkadotapp.feature_dotns_gateway_impl

import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.DotNsReservationMessage
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.model.toScale
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.extension.DotNsRegisterProofMessage
import org.junit.Assert.assertEquals
import org.junit.Test

class DotNsScaleConformanceTest {
    @Test
    fun `reservation message with reserved label matches pallet encoding`() {
        val payload = DotNsReservationMessage.signingPayload(
            candidate = CANDIDATE,
            attester = ATTESTER,
            usernameBase = "alice",
            chatKey = CHAT_KEY,
            reservedBaseLabel = "alice",
            signedAt = SIGNED_AT
        )

        val expected = "64706f703a646f746e732d676174657761793a72657365727665" +
            "0101010101010101010101010101010101010101010101010101010101010101" +
            "0202020202020202020202020202020202020202020202020202020202020202" +
            "14616c696365" +
            "0501" + "03".repeat(65) +
            "01" + "14616c696365" +
            "00f1536500000000"

        assertEquals(expected, payload.toHexString())
    }

    @Test
    fun `reservation message without reserved label encodes None option`() {
        val payload = DotNsReservationMessage.signingPayload(
            candidate = CANDIDATE,
            attester = ATTESTER,
            usernameBase = "alice",
            chatKey = CHAT_KEY,
            reservedBaseLabel = null,
            signedAt = SIGNED_AT
        )

        val expected = "64706f703a646f746e732d676174657761793a72657365727665" +
            "0101010101010101010101010101010101010101010101010101010101010101" +
            "0202020202020202020202020202020202020202020202020202020202020202" +
            "14616c696365" +
            "0501" + "03".repeat(65) +
            "00" +
            "00f1536500000000"

        assertEquals(expected, payload.toHexString())
    }

    @Test
    fun `link LiteUsername encodes as variant 0 with compact-prefixed label`() {
        val link = DotNsLink.LiteUsername("alice.42")

        assertEquals("0020616c6963652e3432", link.toScale().scaleEncodeBinary().toHexString())
    }

    @Test
    fun `link None encodes as variant 1 with fixed 65-byte chat key`() {
        val link = DotNsLink.None(CHAT_KEY)

        assertEquals("01" + "03".repeat(65), link.toScale().scaleEncodeBinary().toHexString())
    }

    @Test
    fun `register proof message hashes SCALE tuple of who, label and link`() {
        val hash = DotNsRegisterProofMessage.hash(
            who = CANDIDATE,
            label = "alice",
            link = DotNsLink.LiteUsername("alice.42")
        )

        assertEquals("453cb2a8a3c047ed26b8d0f2596f0527ed98f3668e3ff142923bf3a022129a55", hash.toHexString())
    }

    private companion object {
        val CANDIDATE = ByteArray(32) { 1 }.toDataByteArray()
        val ATTESTER = ByteArray(32) { 2 }.toDataByteArray()
        val CHAT_KEY = ByteArray(65) { 3 }.toDataByteArray()
        const val SIGNED_AT = 1_700_000_000L
    }
}
