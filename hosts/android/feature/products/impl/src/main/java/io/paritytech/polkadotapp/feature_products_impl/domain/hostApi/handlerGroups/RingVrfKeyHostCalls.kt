package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import com.google.gson.annotations.JsonAdapter
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ListRingVrfKeysError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisterRingVrfKeyError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfKeyDisclosure
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfSignError
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.DerivationIndexWire
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.DerivationIndexWireAdapter
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.ProductAccountIdTupleAdapter
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.toDomain
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.HostCallException

/**
 * RFC-0024 key management. Registration only ever touches the caller's own domain, and consuming a
 * foreign key is governed by the permission model, so no capability flag gates these calls.
 */
class RingVrfKeyHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<RegisterRingVrfKeyParams, RingVrfPublicKeyWire>("registerRingVrfKey") { params ->
            callingProductIdProvider.getProductId().flatMap { callingProductId ->
                botApi.registerRingVrfKey(
                    callingProductId,
                    params.index.toDomain().getOrThrow(),
                    params.ring.toDomain(),
                )
            }
                .map { RingVrfPublicKeyWire(it.value.toHexString(withPrefix = true)) }
                .mapRegisterRingVrfKeyError()
        }

        bridge.registerHandler<ListRingVrfKeysParams, List<RegisteredRingVrfKeyWire>>("listRingVrfKeys") { params ->
            callingProductIdProvider.getProductId().flatMap { callingProductId ->
                botApi.listRingVrfKeys(
                    callingProductId,
                    ProductId.fromStoredValue(params.owner),
                    params.disclosure.toDisclosure(),
                )
            }
                .map { entries -> entries.map { it.toWire() } }
                .mapListRingVrfKeysError()
        }

        bridge.registerHandler<RingVrfSignParams, RingVrfSignatureWire>("ringVrfSign") { params ->
            callingProductIdProvider.getProductId().flatMap { callingProductId ->
                botApi.ringVrfSign(
                    callingProductId,
                    params.keyHandle,
                    DataByteArray.fromHex(params.message).value,
                )
            }
                .map { RingVrfSignatureWire(it.toHexString(withPrefix = true)) }
                .mapRingVrfSignError()
        }
    }
}

private fun String.toDisclosure(): RingVrfKeyDisclosure = when (this) {
    "Anonymized" -> RingVrfKeyDisclosure.ANONYMIZED
    "PublicKey" -> RingVrfKeyDisclosure.PUBLIC_KEY
    else -> throw HostCallException("Unknown", "Unknown ring VRF key disclosure: $this")
}

private fun RegisteredRingVrfKey.toWire(): RegisteredRingVrfKeyWire = RegisteredRingVrfKeyWire(
    handle = handle,
    rings = rings.map { ring ->
        RingLocationResponse(
            chainId = ring.chainId.value.toHexString(withPrefix = true),
            junctions = ring.junctions.map { it.toResponse() },
        )
    },
    publicKey = publicKey?.value?.toHexString(withPrefix = true),
)

private fun RingLocationJunction.toResponse(): RingLocationJunctionResponse = when (this) {
    is RingLocationJunction.PalletInstance -> RingLocationJunctionResponse("PalletInstance", index.toInt().toString())
    is RingLocationJunction.CollectionId ->
        RingLocationJunctionResponse("CollectionId", bytes.value.toHexString(withPrefix = true))
}

private fun <T> Result<T>.mapRegisterRingVrfKeyError(): Result<T> = recoverCatching { throwable ->
    val code = when (throwable) {
        is RegisterRingVrfKeyError.NotConnected -> "NotConnected"
        is RegisterRingVrfKeyError.RingNotFound -> "RingNotFound"
        is RegisterRingVrfKeyError.Rejected -> "Rejected"
        else -> "Unknown"
    }
    throw HostCallException(code, throwable.message ?: code)
}

private fun <T> Result<T>.mapListRingVrfKeysError(): Result<T> = recoverCatching { throwable ->
    val code = when (throwable) {
        is ListRingVrfKeysError.NotConnected -> "NotConnected"
        is ListRingVrfKeysError.Rejected -> "Rejected"
        else -> "Unknown"
    }
    throw HostCallException(code, throwable.message ?: code)
}

private fun <T> Result<T>.mapRingVrfSignError(): Result<T> = recoverCatching { throwable ->
    val code = when (throwable) {
        is RingVrfSignError.NotConnected -> "NotConnected"
        is RingVrfSignError.KeyNotRegistered -> "KeyNotRegistered"
        is RingVrfSignError.NotAllowlisted -> "NotAllowlisted"
        is RingVrfSignError.Rejected -> "Rejected"
        else -> "Unknown"
    }
    throw HostCallException(code, throwable.message ?: code)
}

private data class RegisterRingVrfKeyParams(
    @JsonAdapter(DerivationIndexWireAdapter::class)
    val index: DerivationIndexWire,
    val ring: RingLocationWire,
)
private data class RingVrfPublicKeyWire(val publicKey: HexString)

private data class ListRingVrfKeysParams(val owner: String, val disclosure: String)

private data class RingVrfSignParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val keyHandle: ProductAccountId,
    val message: HexString,
)
private data class RingVrfSignatureWire(val signature: HexString)

private data class RingLocationJunctionResponse(val tag: String, val value: String)
private data class RingLocationResponse(val chainId: HexString, val junctions: List<RingLocationJunctionResponse>)
private data class RegisteredRingVrfKeyWire(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val handle: ProductAccountId,
    val rings: List<RingLocationResponse>,
    val publicKey: HexString?,
)
