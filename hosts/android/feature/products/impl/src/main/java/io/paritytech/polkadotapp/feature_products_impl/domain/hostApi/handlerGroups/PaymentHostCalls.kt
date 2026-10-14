package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import com.google.gson.annotations.JsonAdapter
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.DerivationIndexWire
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.DerivationIndexWireAdapter
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.toDomain
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.HostCallException
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestError
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ProductPaymentRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpError
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.map
import java.math.BigInteger

class PaymentHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerSubscription<Unit, PaymentBalanceDto>("paymentBalanceSubscribe") {
            flowOfAll {
                val productId = callingProductIdProvider.getProductId().getOrThrow()
                botApi.subscribePaymentBalance(productId)
                    .map { PaymentBalanceDto(available = it.available.value.toString()) }
            }
        }

        bridge.registerHandler<PaymentRequestParams, Unit>("paymentRequest") { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            val amount = BigInteger(params.amount).intoBalance()
            val destination = params.destinationHex.fromHex().intoAccountId()

            ProductPaymentRequestId.fromHex(params.idHex)
                .flatMap { id -> botApi.requestPayment(productId, id, amount, destination) }
                .mapPaymentRequestError()
        }

        bridge.registerHandler<PaymentTopUpParams, Unit>("paymentTopUp") { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            val amount = BigInteger(params.amount).intoBalance()
            val source = params.toDomainSource()

            val id = params.topUpId().getOrThrow()

            botApi.topUp(productId, id, amount, source).mapTopUpError()
        }

        bridge.registerSubscription<PaymentTopUpStatusParams, PaymentTopUpStatusDto>("paymentTopUpStatusSubscribe") { params ->
            flowOfAll {
                val productId = callingProductIdProvider.getProductId().getOrThrow()
                botApi.subscribeTopUpStatus(productId, params.topUpId().getOrThrow())
                    .map { it.toDto() }
                    .catch { throw it.asTopUpHostCall() }
            }
        }

        bridge.registerSubscription<PaymentStatusSubscribeParams, PaymentStatusDto>("paymentStatusSubscribe") { params ->
            flowOfAll {
                val productId = callingProductIdProvider.getProductId().getOrThrow()
                botApi.subscribePaymentStatus(productId, ProductPaymentRequestId.fromHex(params.idHex).getOrThrow())
                    .map { it.toDto() }
                    .catch { throw it.asPaymentRequestHostCall() }
            }
        }
    }
}

private fun PaymentStatus.toDto(): PaymentStatusDto = when (this) {
    PaymentStatus.Processing -> PaymentStatusDto(tag = "Processing", value = null)
    PaymentStatus.Completed -> PaymentStatusDto(tag = "Completed", value = null)
    is PaymentStatus.PartiallyClaimed -> PaymentStatusDto(tag = "PartiallyClaimed", value = claimed.value.toString())
    is PaymentStatus.Failed -> PaymentStatusDto(tag = "Failed", value = reason)
}

private fun TopUpStatus.toDto(): PaymentTopUpStatusDto = when (this) {
    TopUpStatus.Detecting -> PaymentTopUpStatusDto(tag = "Detecting")
    TopUpStatus.Claiming -> PaymentTopUpStatusDto(tag = "Claiming")
    is TopUpStatus.Claimed -> PaymentTopUpStatusDto(tag = "Claimed", finalized = finalized)

    is TopUpStatus.ClaimedPartially ->
        PaymentTopUpStatusDto(tag = "ClaimedPartially", actualClaimed = actualClaimed.value.toString())

    TopUpStatus.NotClaimed -> PaymentTopUpStatusDto(tag = "NotClaimed")
}

private fun <T> Result<T>.mapPaymentRequestError(): Result<T> = mapError { it.asPaymentRequestHostCall() }

private fun Throwable.asPaymentRequestHostCall(): HostCallException {
    val code = when (this) {
        is PaymentRequestError.AlreadyExists -> "AlreadyExists"
        is PaymentRequestError.Rejected -> "Rejected"
        is PaymentRequestError.InsufficientBalance -> "InsufficientBalance"
        is PaymentRequestError.NotFound -> "NotFound"
        else -> "Unknown"
    }

    return HostCallException(code, message ?: code)
}

private fun <T> Result<T>.mapTopUpError(): Result<T> = recoverCatching { throw it.asTopUpHostCall() }

private fun Throwable.asTopUpHostCall(): HostCallException {
    val code = when (this) {
        is TopUpError.InvalidSource -> "InvalidSource"
        is TopUpError.AlreadyExists -> "AlreadyExists"
        is TopUpError.SourceBusy -> "SourceBusy"
        is TopUpError.NotFound -> "NotFound"
        else -> "Unknown"
    }

    return HostCallException(code, message ?: code)
}

private data class PaymentBalanceDto(val available: String)

private data class PaymentRequestParams(
    /** Amount in planks, as a decimal string (to preserve u128 precision across JSON). */
    val amount: String,
    val destinationHex: HexString,
    /** The product's own id for this payment; the only thing that names it afterwards. 32 bytes, as hex. */
    val idHex: HexString,
)

private data class PaymentTopUpParams(
    /** The product's own id for this top-up; the only thing that names it afterwards. 32 bytes, as hex. */
    val id: HexString,
    /** Amount in planks, as a decimal string (to preserve u128 precision across JSON). */
    val amount: String,
    /** "ProductAccount", "PrivateKey" or "Coins" — discriminator for the flattened source fields below. */
    val sourceTag: String,
    @JsonAdapter(DerivationIndexWireAdapter::class)
    val sourceDerivationIndex: DerivationIndexWire? = null,
    val sourceKeyHex: HexString? = null,
    val sourceKeyListHex: List<HexString>? = null,
) {
    fun topUpId() = PaymentTopUpId.fromBytes(DataByteArray(id.fromHex()))

    fun toDomainSource(): PaymentTopUpSource = when (sourceTag) {
        "ProductAccount" -> PaymentTopUpSource.ProductAccount(
            index = requireNotNull(sourceDerivationIndex) {
                "sourceDerivationIndex missing for ProductAccount source"
            }.toDomain().getOrThrow(),
        )
        "PrivateKey" -> PaymentTopUpSource.PrivateKey(
            DataByteArray(requireNotNull(sourceKeyHex) { "sourceKeyHex missing for PrivateKey source" }.fromHex())
        )
        "Coins" -> PaymentTopUpSource.Coins(
            secretKeys = requireNotNull(sourceKeyListHex) { "sourceKeyListHex missing for Coins source" }
                .map { DataByteArray(it.fromHex()) }
        )
        else -> throw IllegalArgumentException("Unknown top-up source tag: $sourceTag")
    }
}

private data class PaymentTopUpStatusParams(val id: HexString) {
    fun topUpId() = PaymentTopUpId.fromBytes(DataByteArray(id.fromHex()))
}

private data class PaymentTopUpStatusDto(
    val tag: String,
    /** Only on "Claimed". */
    val finalized: Boolean? = null,
    /** Only on "ClaimedPartially"; planks, as a decimal string. */
    val actualClaimed: String? = null,
)

private data class PaymentStatusSubscribeParams(val idHex: HexString)

/** `value` is the reason on "Failed" and the claimed planks, as a decimal string, on "PartiallyClaimed". */
private data class PaymentStatusDto(val tag: String, val value: String?)
