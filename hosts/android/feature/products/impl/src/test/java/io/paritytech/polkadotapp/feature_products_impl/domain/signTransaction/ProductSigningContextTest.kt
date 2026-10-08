package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.WithdrawnByCaller
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductSigningContextTest {
    private val account = ProductAccountId("lottery.dot", DerivationIndex32.default())
    private val body = SigningRequestBody.SignVrf(
        account = account,
        transcriptLabel = "label".toByteArray(),
        items = listOf(VrfTranscriptItem(label = "l".toByteArray().toDataByteArray(), value = byteArrayOf(1).toDataByteArray())),
    )
    private val context = ProductSigningContext("lottery.dot", "", body, SigningAccount.Product(account))

    @Test
    fun `approving signs nothing when the caller already withdrew the request`() = runTest {
        val caller = launch { context.awaitResult {} }
        runCurrent()
        caller.cancel()
        runCurrent()
        var signed = false

        val result = context.approve { Result.failure<SignedTransaction>(IllegalStateException()).also { signed = true } }

        assertFalse(signed)
        assertTrue("expected a silent withdrawal but was ${result.exceptionOrNull()}", result.exceptionOrNull() is WithdrawnByCaller)
    }

    /** A signing sheet popped from outside must not leave its caller waiting forever. */
    @Test
    fun `the caller is answered with a failure when the sheet goes away unanswered`() = runTest {
        val result = async { context.awaitResult {} }
        runCurrent()

        context.onAbandoned()

        assertTrue(result.await().isFailure)
    }
}
