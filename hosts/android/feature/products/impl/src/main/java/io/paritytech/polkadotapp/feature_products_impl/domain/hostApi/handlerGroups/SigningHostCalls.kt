package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import com.google.gson.annotations.JsonAdapter
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAccountId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.signing.RawPayloadContent
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignerPayloadJson
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRawLegacyPayload
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRawPayload
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_api.model.signing.createTransaction.TxPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.ProductAccountIdTupleAdapter
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.EncodedTransactionExtensionValue

class SigningHostCalls(
    private val botApi: ProductsBotApi,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<SignPayloadParams, SignResultResponse>("signPayload") { params ->
            val signingRequestBody = SigningRequestBody.Transaction(params.toDomain())
            botApi.signPayload(signingRequestBody).map {
                SignResultResponse(
                    signature = it.signature.value.toHexString(withPrefix = true),
                    signedTx = it.signedTx.value.toHexString(withPrefix = true)
                )
            }
        }

        bridge.registerHandler<SignRawParams, SignResultResponse>("signRaw") { params ->
            val payload = SigningRawPayload(params.account, params.toSignRawPayload())
            val signingRequestBody = SigningRequestBody.Raw(payload)
            botApi.signRaw(signingRequestBody).map {
                SignResultResponse(signature = it.signature.value.toHexString(withPrefix = true), signedTx = null)
            }
        }

        bridge.registerHandler<CreateTransactionParams, CreateTransactionResponse>("createTransaction") { params ->
            val signingRequestBody = SigningRequestBody.CreateTransaction(params.toDomain())
            botApi.signCreateTransaction(signingRequestBody).map {
                CreateTransactionResponse(signedTx = it.signedTx.value.toHexString(withPrefix = true))
            }
        }

        bridge.registerHandler<SignRawLegacyParams, SignResultResponse>("signRawWithLegacyAccount") { params ->
            val payload = SigningRawLegacyPayload(params.toAccountId(), params.toSignRawPayload())
            val signingRequestBody = SigningRequestBody.RawLegacy(payload)
            botApi.signRawLegacy(signingRequestBody).map {
                SignResultResponse(signature = it.signature.value.toHexString(withPrefix = true), signedTx = null)
            }
        }

        bridge.registerHandler<CreateTransactionLegacyParams, CreateTransactionResponse>("createTransactionWithLegacyAccount") { params ->
            val signingRequestBody = SigningRequestBody.CreateTransactionLegacy(params.toDomain())
            botApi.signCreateTransactionLegacy(signingRequestBody).map {
                CreateTransactionResponse(signedTx = it.signedTx.value.toHexString(withPrefix = true))
            }
        }
    }
}

private data class SignPayloadParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val account: ProductAccountId,
    val blockHash: HexString,
    val blockNumber: HexString,
    val era: HexString,
    val genesisHash: HexString,
    val method: HexString,
    val nonce: HexString,
    val specVersion: HexString,
    val tip: HexString,
    val transactionVersion: HexString,
    val signedExtensions: List<String>,
    val version: Int,
    val assetId: HexString?,
    val metadataHash: HexString?,
    val mode: Int?,
    val withSignedTransaction: Boolean?,
) {
    fun toDomain(): SignerPayloadJson<ProductAccountId> = SignerPayloadJson(
        account = account,
        blockHash = blockHash.fromHex(),
        blockNumber = blockNumber.fromHex(),
        era = era.fromHex(),
        genesisHash = genesisHash.fromHex(),
        method = method.fromHex(),
        nonce = nonce.fromHex(),
        specVersion = specVersion.fromHex(),
        tip = tip.fromHex(),
        transactionVersion = transactionVersion.fromHex(),
        signedExtensions = signedExtensions,
        version = version,
        assetId = assetId?.fromHex(),
        metadataHash = metadataHash?.fromHex(),
        mode = mode,
        withSignedTransaction = withSignedTransaction,
    )
}

private data class SignRawParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val account: ProductAccountId,
    val data: HexString?,
    val payload: String?,
) {
    fun toSignRawPayload(): RawPayloadContent = rawPayloadContentOf(data, payload)
}

private fun rawPayloadContentOf(data: HexString?, payload: String?): RawPayloadContent = when {
    data != null -> RawPayloadContent.Bytes(data.fromHex())
    payload != null -> RawPayloadContent.Payload(payload)
    else -> error("Raw signing request must have either data or payload")
}

private data class CreateTransactionParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val signer: ProductAccountId,
    val genesisHash: HexString,
    val callData: HexString,
    val extensions: List<TxPayloadExtensionParams>,
    val txExtVersion: Int,
) {
    fun toDomain(): TxPayload<ProductAccountId> = TxPayload(
        signer = signer,
        genesisHash = genesisHash.hexToDataByteArray(),
        callData = callData.hexToDataByteArray(),
        extensions = extensions.map { it.toDomain() },
        txExtVersion = txExtVersion.toUByte(),
    )
}

private data class SignRawLegacyParams(
    val account: String,
    val data: HexString?,
    val payload: String?,
) {
    fun toAccountId(): AccountId = account.parseSigner()

    fun toSignRawPayload(): RawPayloadContent = rawPayloadContentOf(data, payload)
}

private fun String.parseSigner(): AccountId {
    return runCatching { toAccountId().intoAccountId() }
        .recover { fromHex().intoAccountId() }
        .getOrThrow()
}

private data class CreateTransactionLegacyParams(
    val signer: HexString,
    val genesisHash: HexString,
    val callData: HexString,
    val extensions: List<TxPayloadExtensionParams>,
    val txExtVersion: Int,
) {
    fun toDomain(): TxPayload<AccountId> = TxPayload(
        signer = signer.parseSigner(),
        genesisHash = genesisHash.hexToDataByteArray(),
        callData = callData.hexToDataByteArray(),
        extensions = extensions.map { it.toDomain() },
        txExtVersion = txExtVersion.toUByte(),
    )
}

private data class TxPayloadExtensionParams(
    val id: String,
    val implicit: HexString,
    val explicit: HexString,
) {
    fun toDomain(): EncodedTransactionExtensionValue = EncodedTransactionExtensionValue(
        id = id,
        implicit = implicit.hexToDataByteArray(),
        explicit = explicit.hexToDataByteArray(),
    )
}

private data class SignResultResponse(val signature: HexString, val signedTx: HexString?)

private data class CreateTransactionResponse(val signedTx: HexString)
