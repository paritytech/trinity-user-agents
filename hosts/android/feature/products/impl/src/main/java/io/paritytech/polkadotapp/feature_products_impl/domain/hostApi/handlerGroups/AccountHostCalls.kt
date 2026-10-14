package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import com.google.gson.JsonElement
import com.google.gson.annotations.JsonAdapter
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.CreateProofError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.GetAliasError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfProof
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SignVrfError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
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

class AccountHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<AccountGetParams, ProductAccountResponse>("accountGet") { params ->
            botApi.accountGet(callingProductIdProvider.getProductId().getOrThrow(), params.account)
                .map { ProductAccountResponse(it.publicKey) }
        }

        bridge.registerHandler<AccountGetAliasParams, ContextualAliasWire>("accountGetAlias") { params ->
            botApi.accountGetAlias(
                callingProductIdProvider.getProductId().getOrThrow(),
                params.keyHandle,
                params.context.toDomain(),
                params.ring.toDomain(),
            )
                .map { it.toWire() }
                .mapGetAliasError()
        }

        bridge.registerHandler<AccountCreateProofParams, RingVrfProofWire>("accountCreateProof") { params ->
            botApi.accountCreateProof(
                callingProductIdProvider.getProductId().getOrThrow(),
                params.keyHandle,
                params.context.toDomain(),
                params.ring.toDomain(),
                DataByteArray.fromHex(params.message).value,
            )
                .map { it.toWire() }
                .mapCreateProofError()
        }

        bridge.registerHandler<AccountSignVrfParams, VrfSignatureWire>("accountSignVrf") { params ->
            callingProductIdProvider.getProductId().flatMap { callingProductId ->
                botApi.accountSignVrf(
                    callingProductId,
                    params.account,
                    DataByteArray.fromHex(params.label).value,
                    params.items.map { it.toDomain() },
                )
            }
                .map { it.toWire() }
                .mapSignVrfError()
        }

        bridge.registerHandler<Unit, List<LegacyAccountResponse>>("getLegacyAccounts") {
            botApi.getLegacyAccounts().map { accounts ->
                accounts.map { LegacyAccountResponse(it.publicKey, it.name) }
            }
        }
    }
}

private fun <T> Result<T>.mapGetAliasError(): Result<T> = recoverCatching { throwable ->
    val code = when (throwable) {
        is GetAliasError.RingNotFound -> "RingNotFound"
        is GetAliasError.NotMember -> "NotMember"
        is GetAliasError.KeyNotRegistered -> "KeyNotRegistered"
        is GetAliasError.KeyNotInRing -> "KeyNotInRing"
        is GetAliasError.Rejected -> "Rejected"
        else -> "Unknown"
    }
    throw HostCallException(code, throwable.message ?: code)
}

private fun <T> Result<T>.mapCreateProofError(): Result<T> = recoverCatching { throwable ->
    val code = when (throwable) {
        is CreateProofError.RingNotFound -> "RingNotFound"
        is CreateProofError.NotMember -> "NotMember"
        is CreateProofError.KeyNotRegistered -> "KeyNotRegistered"
        is CreateProofError.KeyNotInRing -> "KeyNotInRing"
        is CreateProofError.NotAllowlisted -> "NotAllowlisted"
        is CreateProofError.Rejected -> "Rejected"
        else -> "Unknown"
    }
    throw HostCallException(code, throwable.message ?: code)
}

// RFC-0023 error variants; NotConnected is not reachable — there is no session gate on this host.
private fun <T> Result<T>.mapSignVrfError(): Result<T> = recoverCatching { throwable ->
    val code = when (throwable) {
        is SignVrfError.Rejected -> "Rejected"
        else -> "Unknown"
    }
    throw HostCallException(code, throwable.message ?: code)
}

private fun VrfTranscriptItemWire.toDomain(): VrfTranscriptItem = VrfTranscriptItem(
    label = DataByteArray.fromHex(label),
    value = DataByteArray.fromHex(value),
)

private fun VrfSignature.toWire(): VrfSignatureWire = VrfSignatureWire(
    preOutput = preOutput.value.toHexString(withPrefix = true),
    proof = proof.value.toHexString(withPrefix = true),
)

private fun ProductProofContextWire.toDomain(): ProductProofContext = ProductProofContext(
    productId = ProductId.fromStoredValue(productId),
    suffix = suffix.toDomain().getOrThrow(),
)

internal fun RingLocationWire.toDomain(): RingLocation = RingLocation(
    chainId = DataByteArray.fromHex(chainId),
    junctions = junctions.map { it.toDomain() },
)

internal fun RingLocationJunctionWire.toDomain(): RingLocationJunction = when (tag) {
    "PalletInstance" -> RingLocationJunction.PalletInstance(requireNotNull(value).asInt.toUByte())
    "CollectionId" -> RingLocationJunction.CollectionId(DataByteArray.fromHex(requireNotNull(value).asString))
    else -> throw HostCallException("Unknown", "Unknown ring location junction tag: $tag")
}

private fun ContextualAlias.toWire(): ContextualAliasWire = ContextualAliasWire(
    context = context.value.toHexString(withPrefix = true),
    alias = alias.value.toHexString(withPrefix = true),
)

private fun RingVrfProof.toWire(): RingVrfProofWire = RingVrfProofWire(
    proof = proof.value.toHexString(withPrefix = true),
    contextualAlias = contextualAlias.toWire(),
    ringIndex = ringIndex.value.toInt(),
    ringRevision = ringRevision.value,
)

private data class AccountGetParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val account: ProductAccountId,
)
private data class ProductAccountResponse(val publicKey: String)
private data class LegacyAccountResponse(val publicKey: String, val name: String?)

private data class ProductProofContextWire(
    val productId: String,
    @JsonAdapter(DerivationIndexWireAdapter::class)
    val suffix: DerivationIndexWire,
)
internal data class RingLocationJunctionWire(val tag: String, val value: JsonElement?)

internal data class RingLocationWire(val chainId: HexString, val junctions: List<RingLocationJunctionWire>)

// `keyHandle` shares the (product, index) shape of an sr25519 product account, but names a slot in
// the owner's ring-VRF domain instead.
private data class AccountGetAliasParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val keyHandle: ProductAccountId,
    val context: ProductProofContextWire,
    val ring: RingLocationWire,
)
private data class AccountCreateProofParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val keyHandle: ProductAccountId,
    val context: ProductProofContextWire,
    val ring: RingLocationWire,
    val message: HexString,
)

private data class VrfTranscriptItemWire(val label: HexString, val value: HexString)
private data class AccountSignVrfParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val account: ProductAccountId,
    val label: HexString,
    val items: List<VrfTranscriptItemWire>,
)
private data class VrfSignatureWire(val preOutput: HexString, val proof: HexString)

private data class ContextualAliasWire(val context: HexString, val alias: HexString)
private data class RingVrfProofWire(
    val proof: HexString,
    val contextualAlias: ContextualAliasWire,
    val ringIndex: Int,
    val ringRevision: Int,
)
