package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.signing.RawPayloadContent
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignerPayloadJson
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRawLegacyPayload
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRawPayload
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_api.model.signing.createTransaction.TxPayload
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.EncodedTransactionExtensionValue
import uniffi.truapi.AllocatableResource
import uniffi.truapi.DerivationIndex
import uniffi.truapi.HostSignPayloadData
import uniffi.truapi.RawPayload
import uniffi.truapi.RingLocation
import uniffi.truapi.TxPayloadExtension
import uniffi.truapi.CreateTransactionReview
import uniffi.truapi.SignPayloadReview
import uniffi.truapi.SignRawReview
import uniffi.truapi.SignVrfReview
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.ProductAccountId as NativeProductAccountId

/**
 * Maps a core [UserConfirmationReview] onto the prompt the app shows for it.
 *
 * Exhaustive on purpose, with no catch-all: a variant added upstream must break
 * this compile rather than silently become a denial, which is how seven of the
 * eleven variants were being refused.
 */
fun UserConfirmationReview.toConfirmation(callingProductId: String): TrUAPIConfirmation = when (this) {
    is UserConfirmationReview.SignPayload ->
        TrUAPIConfirmation.Signing(callingProductId, v1.toSigningRequestBody())
    is UserConfirmationReview.SignRaw ->
        TrUAPIConfirmation.Signing(callingProductId, v1.toSigningRequestBody())
    is UserConfirmationReview.CreateTransaction ->
        TrUAPIConfirmation.Signing(callingProductId, v1.toSigningRequestBody())
    is UserConfirmationReview.SignVrf ->
        TrUAPIConfirmation.Signing(v1.callingProductId, v1.toSigningRequestBody())

    is UserConfirmationReview.StatementStoreProductSign ->
        TrUAPIConfirmation.StatementSign(
            requesterProductId = v1.account.dotNsIdentifier,
            payloadSize = v1.payload.size,
        )

    is UserConfirmationReview.AccountAlias ->
        TrUAPIConfirmation.AccountAlias(
            requesterProductId = v1.callingProductId,
            proofContext = v1.context.productId,
            ring = v1.ringLocation.describe(),
        )

    is UserConfirmationReview.CreateProof ->
        TrUAPIConfirmation.CreateProof(
            requesterProductId = v1.callingProductId,
            proofContext = v1.context.productId,
            ring = v1.ringLocation.describe(),
            messageSize = v1.message.size,
        )

    is UserConfirmationReview.IdentityDisclosure ->
        TrUAPIConfirmation.IdentityDisclosure(requesterProductId = v1.productId)

    is UserConfirmationReview.ResourceAllocation ->
        TrUAPIConfirmation.ResourceAllocation(
            requesterProductId = v1.callingProductId,
            resources = v1.resources.map { it.describe() },
        )

    is UserConfirmationReview.PreimageSubmit ->
        TrUAPIConfirmation.PreimageSubmit(
            requesterProductId = callingProductId,
            sizeBytes = v1.size.toLong(),
        )

    is UserConfirmationReview.AccountAccess ->
        TrUAPIConfirmation.AccountAccess(
            requesterProductId = v1.requestingProductId,
            targetProductId = v1.targetProductId,
        )

    is UserConfirmationReview.ProductSubtree ->
        TrUAPIConfirmation.ProductSubtree(requesterProductId = v1.productId)
}

@OptIn(ExperimentalStdlibApi::class)
private fun RingLocation.describe(): String = chainId.toHexString()

private fun AllocatableResource.describe(): String = when (this) {
    is AllocatableResource.StatementStoreAllowance -> "statement-store allowance"
    is AllocatableResource.BulletinAllowance -> "bulletin allowance"
    is AllocatableResource.SmartContractAllowance -> "smart-contract allowance"
    is AllocatableResource.AutoSigning -> "auto-signing"
}

private fun NativeProductAccountId.toDomain(): ProductAccountId = ProductAccountId(
    productId = dotNsIdentifier,
    index = when (val index = derivationIndex) {
        is DerivationIndex.Index -> DerivationIndex32.fromUInt(index.v1)
        is DerivationIndex.Raw -> DerivationIndex32.fromBytes(index.v1.toDataByteArray())
            .getOrElse { throw UnsupportedReviewException("raw derivation index must be 32 bytes") }
    },
)

private fun <Signer> HostSignPayloadData.toSignerPayloadJson(account: Signer) = SignerPayloadJson(
    account = account,
    blockHash = blockHash,
    blockNumber = blockNumber,
    era = era,
    genesisHash = genesisHash,
    method = method,
    nonce = nonce,
    specVersion = specVersion,
    tip = tip,
    transactionVersion = transactionVersion,
    signedExtensions = signedExtensions,
    version = version.toInt(),
    assetId = assetId,
    metadataHash = metadataHash,
    mode = mode?.toInt(),
    withSignedTransaction = withSignedTransaction,
)

private fun SignPayloadReview.toSigningRequestBody(): SigningRequestBody = when (this) {
    is SignPayloadReview.Product ->
        SigningRequestBody.Transaction(request.payload.toSignerPayloadJson(request.account.toDomain()))
    is SignPayloadReview.LegacyAccount ->
        SigningRequestBody.TransactionLegacy(
            v1.payload.toSignerPayloadJson(v1.signer.parseLegacySigner().toDataByteArray()),
        )
}

private fun RawPayload.toContent(): RawPayloadContent = when (this) {
    is RawPayload.Bytes -> RawPayloadContent.Bytes(bytes)
    is RawPayload.Payload -> RawPayloadContent.Payload(payload)
}

/**
 * An unwatermarked payload carries no `<Bytes>` protection, so a signature over it can authorize a
 * transaction; the core requires a host to say so. This sheet shows a raw payload as an ordinary
 * message and has nowhere to put that warning, and carrying the flag into the domain model would
 * reach the SSO SCALE mappers, which encode a wire message to the paired wallet. So the request is
 * refused rather than presented as the harmless thing it is not; a sheet that can warn lifts this.
 */
private fun SignRawReview.toSigningRequestBody(): SigningRequestBody = when (this) {
    is SignRawReview.Product -> {
        requireWatermark(watermarked)
        SigningRequestBody.Raw(SigningRawPayload(request.account.toDomain(), request.payload.toContent()))
    }
    is SignRawReview.LegacyAccount -> {
        requireWatermark(watermarked)
        SigningRequestBody.RawLegacy(
            SigningRawLegacyPayload(request.signer.parseLegacySigner().toDataByteArray(), request.payload.toContent()),
        )
    }
}

private fun requireWatermark(watermarked: Boolean) {
    if (!watermarked) throw UnsupportedReviewException("raw payload without transaction-payload protection")
}

private fun TxPayloadExtension.toDomain() = EncodedTransactionExtensionValue(
    id = id,
    // Rust: `additional_signed` = implicit (signed, not in body);
    //       `extra` = explicit (in extrinsic body).
    implicit = additionalSigned.toDataByteArray(),
    explicit = extra.toDataByteArray(),
)

private fun CreateTransactionReview.toSigningRequestBody(): SigningRequestBody = when (this) {
    is CreateTransactionReview.Product -> SigningRequestBody.CreateTransaction(
        TxPayload(
            signer = payload.signer.toDomain(),
            genesisHash = payload.genesisHash.toDataByteArray(),
            callData = payload.callData.toDataByteArray(),
            extensions = payload.extensions.map { it.toDomain() },
            txExtVersion = payload.txExtVersion,
        ),
    )
    is CreateTransactionReview.LegacyAccount -> SigningRequestBody.CreateTransactionLegacy(
        TxPayload(
            signer = v1.signer.toDataByteArray(),
            genesisHash = v1.genesisHash.toDataByteArray(),
            callData = v1.callData.toDataByteArray(),
            extensions = v1.extensions.map { it.toDomain() },
            txExtVersion = v1.txExtVersion,
        ),
    )
}

private fun SignVrfReview.toSigningRequestBody(): SigningRequestBody = SigningRequestBody.SignVrf(
    account = request.account.toDomain(),
    transcriptLabel = request.transcriptLabel,
    items = request.items.map { VrfTranscriptItem(it.label.toDataByteArray(), it.value.toDataByteArray()) },
)

/**
 * Parse a hex/SS58 legacy signer string from the wire into a raw AccountId.
 *
 * The non-hex branch must SS58-decode: taking the string's UTF-8 bytes yields a
 * 47-byte value that `ProductRequestAccountResolver` can never match, so every
 * SS58-addressed legacy request would be refused as an unknown account.
 */
private fun String.parseLegacySigner(): ByteArray =
    runCatching { if (startsWith("0x")) fromHex() else toAccountId() }
        .getOrElse { throw UnsupportedReviewException("legacy signer is neither hex nor SS58: $it") }

/** Thrown when a review variant can't be mapped to a signable request. */
class UnsupportedReviewException(what: String) : Exception("unsupported confirmation review: $what")
