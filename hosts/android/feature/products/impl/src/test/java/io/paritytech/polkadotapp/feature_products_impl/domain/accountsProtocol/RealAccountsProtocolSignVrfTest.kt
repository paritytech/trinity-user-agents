package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SignVrfError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.ProductSigningScreenLauncher
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class RealAccountsProtocolSignVrfTest {
    private val signingScreenLauncher: ProductSigningScreenLauncher = mock()

    private val accountsProtocol = RealAccountsProtocol(
        contextHolder = mock(),
        productsRouter = mock(),
        membersRingLocator = mock(),
        membershipProver = mock(),
        bandersnatchSecretsStorage = mock(),
        ringVrfKeyRegistry = mock(),
        ringVrfKeySource = mock(),
        permissionGuard = mock(),
        crossProductProofContextHolder = mock(),
        productSigningScreenLauncher = signingScreenLauncher,
    )

    private val callingProduct = ProductId.fromStoredValue("lottery.dot")
    private val account = ProductAccountId("lottery.dot", DerivationIndex32.default())
    private val label = "pop:airdrop".toByteArray()

    @Test
    fun `rejects more items than the limit`() = runBlocking {
        val result = accountsProtocol.signVrf(callingProduct, account, label, items(count = 33, valueSize = 1))

        assertTrue(result.exceptionOrNull() is SignVrfError.TranscriptTooLarge)
    }

    @Test
    fun `rejects a transcript larger than the limit`() = runBlocking {
        // 8 items x 1 KiB values overshoots the 8 KiB budget once the labels are counted.
        val result = accountsProtocol.signVrf(callingProduct, account, label, items(count = 8, valueSize = 1024))

        assertTrue(result.exceptionOrNull() is SignVrfError.TranscriptTooLarge)
    }

    @Test
    fun `accepts a transcript exactly at the limit`() = runBlocking {
        givenApproved()

        // label + 32 x (5-byte label + value) must land exactly on 8192 bytes.
        val perItemValueSize = (8192 - label.size - 32 * ITEM_LABEL_SIZE) / 32
        val items = items(count = 32, valueSize = perItemValueSize)
        val padding = 8192 - label.size - items.sumOf { it.label.value.size + it.value.value.size }
        val exactItems = items.dropLast(1) + VrfTranscriptItem(
            label = items.last().label,
            value = ByteArray(perItemValueSize + padding).toDataByteArray(),
        )

        val result = accountsProtocol.signVrf(callingProduct, account, label, exactItems)

        assertEquals(PRE_OUTPUT, result.getOrThrow().preOutput.value.first())
    }

    private suspend fun givenApproved() {
        val signature = VrfSignature(
            preOutput = ByteArray(32) { PRE_OUTPUT }.toDataByteArray(),
            proof = ByteArray(64).toDataByteArray(),
        )
        whenever(signingScreenLauncher.awaitDecision(any(), any(), any(), any()))
            .thenReturn(Result.success(SignedTransaction.Vrf(signature)))
    }

    private fun items(count: Int, valueSize: Int): List<VrfTranscriptItem> = List(count) { index ->
        VrfTranscriptItem(
            label = "item$index".take(ITEM_LABEL_SIZE).toByteArray().toDataByteArray(),
            value = ByteArray(valueSize).toDataByteArray(),
        )
    }

    private companion object {
        const val ITEM_LABEL_SIZE = 5
        const val PRE_OUTPUT: Byte = 0x7
    }
}
