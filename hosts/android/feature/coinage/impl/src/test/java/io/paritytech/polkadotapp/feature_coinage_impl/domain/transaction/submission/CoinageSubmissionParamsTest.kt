package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Pins the persisted encoding of the coinage submission policies' parameters.
 *
 * These bytes are stored with every scheduled transaction, so the hex strings below must never be edited. A
 * change that makes the conformance tests fail corrupts rows already written: freeze the old shape as a
 * versioned class, decode both, and add a new test beside these.
 */
@OptIn(ExperimentalTime::class)
class CoinageSubmissionParamsTest {
    @Test
    fun `a transfer with a retry window survives a round trip`() {
        val params = TransferSubmissionParams(buildUntil = RETRY_UNTIL, retryFailures = true)

        val policy = CoinageSubmissionParams.splitPolicy(params)

        assertEquals(params, CoinageSubmissionParams.decodeTransfer(policy.params).getOrThrow())
    }

    /** A transfer built once must not come back as one that is retried. */
    @Test
    fun `a transfer without a retry window survives a round trip`() {
        val params = TransferSubmissionParams(buildUntil = RETRY_UNTIL, retryFailures = false)

        val policy = CoinageSubmissionParams.unloadPolicy(params)

        assertEquals(params, CoinageSubmissionParams.decodeTransfer(policy.params).getOrThrow())
    }

    @Test
    fun `a claim survives a round trip`() {
        val params = ClaimSubmissionParams(retryUntil = RETRY_UNTIL, receivedKey = RECEIVED_KEY)

        val policy = CoinageSubmissionParams.claimPolicy(params)

        assertEquals(params, CoinageSubmissionParams.decodeClaim(policy.params).getOrThrow())
    }

    @Test
    fun `each policy is keyed by its own id`() {
        val transfer = TransferSubmissionParams(RETRY_UNTIL, retryFailures = true)

        assertEquals(COINAGE_SPLIT_POLICY_ID, CoinageSubmissionParams.splitPolicy(transfer).id.value)
        assertEquals(COINAGE_UNLOAD_POLICY_ID, CoinageSubmissionParams.unloadPolicy(transfer).id.value)
        assertEquals(COINAGE_CLAIM_POLICY_ID, CoinageSubmissionParams.claimPolicy(ClaimSubmissionParams(RETRY_UNTIL, RECEIVED_KEY)).id.value)
    }

    // ---- conformance ----

    @Test
    fun `a transfer with a retry window encodes its deadline followed by a set flag`() {
        val encoded = CoinageSubmissionParams.splitPolicy(TransferSubmissionParams(RETRY_UNTIL, retryFailures = true)).params.value

        assertEquals(TRANSFER_WITH_WINDOW, encoded.toHex())
    }

    @Test
    fun `a transfer without a retry window encodes its deadline followed by a cleared flag`() {
        val encoded = CoinageSubmissionParams.splitPolicy(TransferSubmissionParams(RETRY_UNTIL, retryFailures = false)).params.value

        assertEquals(TRANSFER_WITHOUT_WINDOW, encoded.toHex())
    }

    @Test
    fun `a claim encodes its window followed by the received key`() {
        val encoded = CoinageSubmissionParams.claimPolicy(ClaimSubmissionParams(RETRY_UNTIL, RECEIVED_KEY)).params.value

        assertEquals(CLAIM, encoded.toHex())
    }

    @Test
    fun `stored params decode back to what was written`() {
        assertEquals(
            TransferSubmissionParams(RETRY_UNTIL, retryFailures = true),
            CoinageSubmissionParams.decodeTransfer(TRANSFER_WITH_WINDOW.fromHex().toDataByteArray()).getOrThrow(),
        )
        assertEquals(
            TransferSubmissionParams(RETRY_UNTIL, retryFailures = false),
            CoinageSubmissionParams.decodeTransfer(TRANSFER_WITHOUT_WINDOW.fromHex().toDataByteArray()).getOrThrow(),
        )
        assertEquals(
            ClaimSubmissionParams(RETRY_UNTIL, RECEIVED_KEY),
            CoinageSubmissionParams.decodeClaim(CLAIM.fromHex().toDataByteArray()).getOrThrow(),
        )
    }

    /** Corruption is reported rather than swallowed: the policy decides what an unreadable row means. */
    @Test
    fun `unreadable params decode as a failure`() {
        assertTrue(CoinageSubmissionParams.decodeClaim(byteArrayOf(0x01).toDataByteArray()).isFailure)
    }

    private fun ByteArray.toHex() = joinToString("") { "%02x".format(it) }

    private fun String.fromHex() = chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    private companion object {
        val RETRY_UNTIL: Instant = Instant.fromEpochMilliseconds(1_700_000_000_123)
        val RECEIVED_KEY: CoinPrivateKey = byteArrayOf(0x0a, 0x0b, 0x0c).toDataByteArray()

        const val TRANSFER_WITH_WINDOW = "7b68e5cf8b01000001"
        const val TRANSFER_WITHOUT_WINDOW = "7b68e5cf8b01000000"
        const val CLAIM = "7b68e5cf8b0100000c0a0b0c"
    }
}
