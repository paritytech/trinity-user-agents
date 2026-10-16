package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.signing.RawPayloadContent
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.truapi.AllocatableResource
import uniffi.truapi.DerivationIndex
import uniffi.truapi.HostAccountSignVrfRequest
import uniffi.truapi.HostSignPayloadData
import uniffi.truapi.HostSignPayloadRequest
import uniffi.truapi.HostSignPayloadWithLegacyAccountRequest
import uniffi.truapi.HostSignRawRequest
import uniffi.truapi.HostSignRawWithLegacyAccountRequest
import uniffi.truapi.LegacyAccountTxPayload
import uniffi.truapi.ProductAccountTxPayload
import uniffi.truapi.RawPayload
import uniffi.truapi.TxPayloadExtension
import uniffi.truapi.VrfTranscriptItem
import uniffi.truapi.AccountAccessReview
import uniffi.truapi.CreateTransactionReview
import uniffi.truapi.IdentityDisclosureReview
import uniffi.truapi.PreimageSubmitReview
import uniffi.truapi.ProductSubtreeReview
import uniffi.truapi.ResourceAllocationReview
import uniffi.truapi.SignPayloadReview
import uniffi.truapi.SignRawReview
import uniffi.truapi.SignVrfReview
import uniffi.truapi.StatementStoreProductSignReview
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.ProductAccountId as NativeProductAccountId

private const val ALICE_SS58 = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"

class ConfirmationReviewMappingTest {
    private val caller = "caller-product.dot"

    @OptIn(ExperimentalStdlibApi::class)
    private fun aliceAccountId() =
        "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d".hexToByteArray()

    @Test
    fun `sign payload product maps to transaction`() {
        val review = UserConfirmationReview.SignPayload(
            SignPayloadReview.Product(
                callingProductId = caller,
                request = HostSignPayloadRequest(account = nativeAccount(), payload = signPayloadData()),
            ),
        )

        val body = review.signingRequest() as SigningRequestBody.Transaction

        assertEquals(ProductAccountId("demo-product.dot", DerivationIndex32.fromUInt(7u)), body.payload.account)
        assertArrayEquals(byteArrayOf(0xde.toByte(), 0xad.toByte()), body.payload.method)
        assertEquals(listOf("CheckNonce", "CheckWeight"), body.payload.signedExtensions)
        assertEquals(4, body.payload.version)
        assertEquals(1, body.payload.mode)
        assertEquals(true, body.payload.withSignedTransaction)
    }

    /** The core exposes this method, so refusing the variant denied it silently. */
    @Test
    fun `sign payload legacy hex signer maps to transaction legacy`() {
        val body = legacySignPayload(signer = "0x0102") as SigningRequestBody.TransactionLegacy

        assertArrayEquals(byteArrayOf(1, 2), body.payload.account.value)
        assertArrayEquals(byteArrayOf(0xde.toByte(), 0xad.toByte()), body.payload.method)
        assertEquals(listOf("CheckNonce", "CheckWeight"), body.payload.signedExtensions)
        assertEquals(4, body.payload.version)
    }

    /** The wire carries "SS58 or hex"; UTF-8 bytes of an SS58 string match nothing. */
    @Test
    fun `sign payload legacy ss58 signer decodes to an account id`() {
        val body = legacySignPayload(signer = ALICE_SS58) as SigningRequestBody.TransactionLegacy

        assertArrayEquals(aliceAccountId(), body.payload.account.value)
    }

    @Test
    fun `a legacy signer that is neither hex nor ss58 is unsupported`() {
        assertThrows(UnsupportedReviewException::class.java) { legacySignPayload(signer = "not-an-address") }
    }

    private fun legacySignPayload(signer: String): SigningRequestBody =
        UserConfirmationReview.SignPayload(
            SignPayloadReview.LegacyAccount(
                HostSignPayloadWithLegacyAccountRequest(signer = signer, payload = signPayloadData()),
            ),
        ).signingRequest()

    @Test
    fun `sign raw product bytes maps to raw`() {
        val review = UserConfirmationReview.SignRaw(
            SignRawReview.Product(
                callingProductId = caller,
                request = HostSignRawRequest(
                    account = nativeAccount(),
                    payload = RawPayload.Bytes(byteArrayOf(0xca.toByte(), 0xfe.toByte())),
                ),
                watermarked = true,
            ),
        )

        val body = review.signingRequest() as SigningRequestBody.Raw

        assertEquals(ProductAccountId("demo-product.dot", DerivationIndex32.fromUInt(7u)), body.payload.account)
        val content = body.payload.type as RawPayloadContent.Bytes
        assertArrayEquals(byteArrayOf(0xca.toByte(), 0xfe.toByte()), content.data)
    }

    @Test
    fun `sign raw legacy hex signer maps to raw legacy`() {
        val review = UserConfirmationReview.SignRaw(
            SignRawReview.LegacyAccount(
                request = HostSignRawWithLegacyAccountRequest(
                    signer = "0x0102",
                    payload = RawPayload.Payload("hello"),
                ),
                watermarked = true,
            ),
        )

        val body = review.signingRequest() as SigningRequestBody.RawLegacy

        assertArrayEquals(byteArrayOf(1, 2), body.payload.account.value)
        assertEquals("hello", (body.payload.type as RawPayloadContent.Payload).data)
    }

    // The core says a host must warn that a signature over an unwatermarked payload can authorize a
    // transaction. This sheet shows a raw payload as a message and cannot say that, so the request is
    // refused instead of being shown as a harmless one.
    @Test
    fun `a raw payload with no transaction-payload protection is refused, not shown as a message`() {
        val product = UserConfirmationReview.SignRaw(
            SignRawReview.Product(
                callingProductId = caller,
                request = HostSignRawRequest(account = nativeAccount(), payload = RawPayload.Payload("hello")),
                watermarked = false,
            ),
        )
        val legacy = UserConfirmationReview.SignRaw(
            SignRawReview.LegacyAccount(
                request = HostSignRawWithLegacyAccountRequest(signer = "0x0102", payload = RawPayload.Payload("hello")),
                watermarked = false,
            ),
        )

        assertThrows(UnsupportedReviewException::class.java) { product.signingRequest() }
        assertThrows(UnsupportedReviewException::class.java) { legacy.signingRequest() }
    }

    @Test
    fun `create transaction product maps extensions`() {
        val review = UserConfirmationReview.CreateTransaction(
            CreateTransactionReview.Product(
                callingProductId = caller,
                payload = ProductAccountTxPayload(
                    signer = nativeAccount(),
                    genesisHash = ByteArray(32) { 3 },
                    callData = byteArrayOf(9),
                    extensions = listOf(
                        TxPayloadExtension(
                            id = "CheckNonce",
                            extra = byteArrayOf(1),
                            additionalSigned = byteArrayOf(2),
                        ),
                    ),
                    txExtVersion = 0u,
                    contacts = emptyList(),
                ),
            ),
        )

        val body = review.signingRequest() as SigningRequestBody.CreateTransaction

        assertEquals(ProductAccountId("demo-product.dot", DerivationIndex32.fromUInt(7u)), body.payload.signer)
        val extension = body.payload.extensions.single()
        assertEquals("CheckNonce", extension.id)
        assertArrayEquals(byteArrayOf(1), extension.explicit.value)
        assertArrayEquals(byteArrayOf(2), extension.implicit.value)
    }

    @Test
    fun `create transaction legacy maps raw signer`() {
        val review = UserConfirmationReview.CreateTransaction(
            CreateTransactionReview.LegacyAccount(
                LegacyAccountTxPayload(
                    signer = ByteArray(32) { 5 },
                    genesisHash = ByteArray(32) { 3 },
                    callData = byteArrayOf(9),
                    extensions = emptyList(),
                    txExtVersion = 0u,
                ),
            ),
        )

        val body = review.signingRequest() as SigningRequestBody.CreateTransactionLegacy

        assertArrayEquals(ByteArray(32) { 5 }, body.payload.signer.value)
    }

    @Test
    fun `sign vrf maps transcript`() {
        val review = UserConfirmationReview.SignVrf(
            SignVrfReview(
                callingProductId = "caller-product.dot",
                request = HostAccountSignVrfRequest(
                    account = nativeAccount(),
                    transcriptLabel = "pop:airdrop".toByteArray(),
                    items = listOf(
                        VrfTranscriptItem(label = "round".toByteArray(), value = byteArrayOf(1)),
                    ),
                ),
            ),
        )

        val body = review.signingRequest() as SigningRequestBody.SignVrf

        assertEquals(ProductAccountId("demo-product.dot", DerivationIndex32.fromUInt(7u)), body.account)
        assertArrayEquals("pop:airdrop".toByteArray(), body.transcriptLabel)
        val item = body.items.single()
        assertArrayEquals("round".toByteArray(), item.label.value)
        assertArrayEquals(byteArrayOf(1), item.value.value)
    }

    @Test
    fun `raw 32-byte derivation index maps to a raw selector`() {
        val review = signVrfReview(derivationIndex = DerivationIndex.Raw(ByteArray(32) { 9 }))

        val body = review.signingRequest() as SigningRequestBody.SignVrf

        val expected = DerivationIndex32.fromBytes(ByteArray(32) { 9 }.toDataByteArray()).getOrThrow()
        assertEquals(expected, body.account.index)
    }

    @Test
    fun `raw derivation index of the wrong length is unsupported`() {
        val review = signVrfReview(derivationIndex = DerivationIndex.Raw(ByteArray(31)))

        assertThrows(UnsupportedReviewException::class.java) { review.toConfirmation(caller) }
    }

    private fun signVrfReview(derivationIndex: DerivationIndex) = UserConfirmationReview.SignVrf(
        SignVrfReview(
            callingProductId = "caller-product.dot",
            request = HostAccountSignVrfRequest(
                account = NativeProductAccountId(
                    dotNsIdentifier = "demo-product.dot",
                    derivationIndex = derivationIndex,
                ),
                transcriptLabel = ByteArray(0),
                items = emptyList(),
            ),
        ),
    )

    @Test
    fun `statement store sign maps to a statement confirmation`() {
        val review = UserConfirmationReview.StatementStoreProductSign(
            StatementStoreProductSignReview(
                callingProductId = caller,
                account = nativeAccount(),
                payload = byteArrayOf(1, 2, 3),
            ),
        )

        val confirmation = review.toConfirmation(caller) as TrUAPIConfirmation.StatementSign

        assertEquals("demo-product.dot", confirmation.requesterProductId)
        assertEquals(3, confirmation.payloadSize)
    }

    @Test
    fun `account access maps both products`() {
        val review = UserConfirmationReview.AccountAccess(
            AccountAccessReview(requestingProductId = "caller.dot", targetProductId = "target.dot"),
        )

        val confirmation = review.toConfirmation(caller) as TrUAPIConfirmation.AccountAccess

        assertEquals("caller.dot", confirmation.requesterProductId)
        assertEquals("target.dot", confirmation.targetProductId)
    }

    @Test
    fun `product subtree maps the product as its own requester`() {
        val review = UserConfirmationReview.ProductSubtree(ProductSubtreeReview(productId = "self.dot"))

        val confirmation = review.toConfirmation(caller) as TrUAPIConfirmation.ProductSubtree

        assertEquals("self.dot", confirmation.requesterProductId)
    }

    @Test
    fun `identity disclosure maps the product`() {
        val review = UserConfirmationReview.IdentityDisclosure(
            IdentityDisclosureReview(productId = "discloser.dot"),
        )

        val confirmation = review.toConfirmation(caller) as TrUAPIConfirmation.IdentityDisclosure

        assertEquals("discloser.dot", confirmation.requesterProductId)
    }

    @Test
    fun `preimage submit carries its size`() {
        val review = UserConfirmationReview.PreimageSubmit(PreimageSubmitReview(size = 4096uL))

        val confirmation = review.toConfirmation(caller) as TrUAPIConfirmation.PreimageSubmit

        assertEquals(4096L, confirmation.sizeBytes)
    }

    @Test
    fun `resource allocation labels every resource`() {
        val review = UserConfirmationReview.ResourceAllocation(
            ResourceAllocationReview(
                callingProductId = "caller.dot",
                resources = listOf(
                    AllocatableResource.StatementStoreAllowance,
                    AllocatableResource.AutoSigning,
                ),
            ),
        )

        val confirmation = review.toConfirmation(caller) as TrUAPIConfirmation.ResourceAllocation

        assertEquals(2, confirmation.resources.size)
    }

    private fun UserConfirmationReview.signingRequest(): SigningRequestBody =
        (toConfirmation(caller) as TrUAPIConfirmation.Signing).request

    private fun nativeAccount() = NativeProductAccountId(
        dotNsIdentifier = "demo-product.dot",
        derivationIndex = DerivationIndex.Index(7u),
    )

    private fun signPayloadData() = HostSignPayloadData(
        blockHash = ByteArray(32) { 1 },
        blockNumber = byteArrayOf(0x2a),
        era = byteArrayOf(0),
        genesisHash = ByteArray(32) { 2 },
        method = byteArrayOf(0xde.toByte(), 0xad.toByte()),
        nonce = byteArrayOf(1),
        specVersion = byteArrayOf(4),
        tip = byteArrayOf(0),
        transactionVersion = byteArrayOf(2),
        signedExtensions = listOf("CheckNonce", "CheckWeight"),
        version = 4u,
        assetId = null,
        metadataHash = null,
        mode = 1u,
        withSignedTransaction = true,
    )
}
