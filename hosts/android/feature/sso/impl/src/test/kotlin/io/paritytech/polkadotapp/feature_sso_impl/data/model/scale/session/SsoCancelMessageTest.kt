package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import org.junit.Assert.assertEquals
import org.junit.Test

private val SESSION_ID = SsoSessionId("session")

class SsoCancelMessageTest {
    /**
     * Built by hand from the pairing host's `RemoteMessage::Cancel(Withdrawal { message_id })`: the envelope id, the
     * V1 tag, variant 24, then the withdrawn request's id as a SCALE string.
     */
    @Test
    fun `a cancel decodes to the request it withdraws when the pairing host sends variant 24`() {
        val encoded = scaleString("cancel-1") + byteArrayOf(0x00, 24) + scaleString("req-7")

        val request = encoded.toSsoSessionRequest(SESSION_ID, requireNotNull(DotNsTld.parse("dot"))).getOrThrow()

        assertEquals("cancel-1", request.requestId)
        assertEquals("req-7", (request.content as SsoSessionRequest.Content.Cancel).messageId)
    }

    private fun scaleString(value: String): ByteArray {
        val bytes = value.toByteArray()
        return byteArrayOf((bytes.size shl 2).toByte()) + bytes
    }
}
