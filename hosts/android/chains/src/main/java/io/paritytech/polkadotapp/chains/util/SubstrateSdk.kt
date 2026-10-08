package io.paritytech.polkadotapp.chains.util

import io.emeraldpay.polkaj.scale.ScaleCodecReader
import io.novasama.substrate_sdk_android.encrypt.seed.SeedFactory
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.RuntimeType
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.Struct
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromByteArrayOrNull
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromHex
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.*
import io.novasama.substrate_sdk_android.runtime.definitions.types.toByteArray
import io.novasama.substrate_sdk_android.runtime.extrinsic.Nonce
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.call
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.CheckGenesis
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.CheckNonce
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.findExtensionOrThrow
import io.novasama.substrate_sdk_android.runtime.metadata.*
import io.novasama.substrate_sdk_android.runtime.metadata.module.*
import io.novasama.substrate_sdk_android.scale.EncodableStruct
import io.novasama.substrate_sdk_android.scale.Schema
import io.novasama.substrate_sdk_android.scale.dataType.DataType
import io.novasama.substrate_sdk_android.ss58.SS58Encoder
import io.paritytech.polkadotapp.chains.multiNetwork.InlineWithRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.chains.network.binding.OriginCaller
import io.paritytech.polkadotapp.chains.network.binding.bindNullableNumberConstant
import io.paritytech.polkadotapp.chains.network.binding.bindNumberConstant
import io.paritytech.polkadotapp.chains.util.Modules.ASSET_REGISTRY
import io.paritytech.polkadotapp.chains.util.Modules.TOKENS
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.ComponentHolder
import io.paritytech.polkadotapp.common.utils.tryFindNonNull
import java.math.BigInteger

fun StorageEntry.splitKeyToComponents(
    runtime: RuntimeSnapshot,
    key: String,
): ComponentHolder {
    return ComponentHolder(splitKey(runtime, key))
}

fun Module.findStorage(vararg candidateNames: String): StorageEntry? {
    return candidateNames.tryFindNonNull { storageOrNull(it) }
}

fun String.extrinsicHash(): String {
    return fromHex().blake2b256().toHexString(withPrefix = true)
}

fun String.hexBytesSize(): Int {
    val contentLength = if (startsWith("0x")) {
        length - 2
    } else {
        length
    }

    return contentLength / 2
}

fun Extrinsic.Instance.tip(): BigInteger? = explicits()?.get(DefaultSignedExtensions.CHECK_TX_PAYMENT) as? BigInteger

fun GenericCall.Instance.instanceOf(functionCandidate: MetadataFunction): Boolean = function == functionCandidate

fun GenericCall.Instance.instanceOf(
    moduleName: String,
    callName: String,
): Boolean = moduleName == module.name && callName == function.name

fun GenericCall.Instance.instanceOf(
    moduleName: String,
    vararg callNames: String,
): Boolean = moduleName == module.name && function.name in callNames

fun GenericEvent.Instance.instanceOf(
    moduleName: String,
    eventName: String,
): Boolean = moduleName == module.name && eventName == event.name

fun GenericEvent.Instance.instanceOf(event: Event): Boolean = event.index == this.event.index

inline fun <reified T> T.scaleEncodeSerializable(): Any? {
    return Scale.encode(this)
}

inline fun <reified T> T.scaleEncodeBinary(): ByteArray {
    return BinaryScale.encodeToByteArray(this)
}

context(withRuntime: WithRuntime)
fun <E> RuntimeType<E, *>.toByteArray(value: E): ByteArray {
    return toByteArray(withRuntime.runtime, value)
}

context(runtime: RuntimeSnapshot)
fun <E> RuntimeType<E, *>.toByteArray(value: E): ByteArray {
    return toByteArray(runtime, value)
}

context(withRuntime: WithRuntime)
fun GenericCall.Instance.toByteArray(): ByteArray {
    return GenericCall.toByteArray(this)
}

context(runtime: RuntimeSnapshot)
fun GenericCall.Instance.toByteArray(): ByteArray {
    return GenericCall.toByteArray(this)
}

fun structOf(vararg pairs: Pair<String, Any?>) = Struct.Instance(mapOf(*pairs))

fun RuntimeSnapshot.composeCall(
    moduleName: String,
    callName: String,
    arguments: Map<String, Any?>
): GenericCall.Instance {
    val module = metadata.module(moduleName)
    val call = module.call(callName)

    return GenericCall.Instance(module, call, arguments)
}

context(withRuntime: WithRuntime)
fun composeCall(
    moduleName: String,
    callName: String,
    arguments: Map<String, Any?>
): GenericCall.Instance {
    return withRuntime.runtime.composeCall(moduleName, callName, arguments)
}

fun ExtrinsicBuilder.call(
    moduleName: String,
    callName: String,
    arguments: EncodedArguments
): ExtrinsicBuilder {
    return call(
        moduleName = moduleName,
        callName = callName,
        arguments = arguments.encoded
    )
}

val SS58Encoder.DEFAULT_PREFIX: Short
    get() = 42.toShort()

fun SeedFactory.deriveSeed32(
    mnemonicWords: String,
    password: String?,
) = cropSeedTo32Bytes(deriveSeed(mnemonicWords, password))

private fun cropSeedTo32Bytes(seedResult: SeedFactory.Result): SeedFactory.Result {
    return SeedFactory.Result(seed = seedResult.seed.copyOfRange(0, 32), seedResult.mnemonic)
}

typealias StructBuilderWithContext<S> = S.(EncodableStruct<S>) -> Unit

operator fun <S : Schema<S>> S.invoke(block: StructBuilderWithContext<S>? = null): EncodableStruct<S> {
    val struct = EncodableStruct(this)

    block?.invoke(this, struct)

    return struct
}

context(withRuntime: WithRuntime)
fun StorageEntry.storageKeyWith(keyArguments: Array<out Any?>): String {
    return if (keyArguments.isEmpty()) {
        storageKey()
    } else {
        storageKey(withRuntime.runtime, *keyArguments)
    }
}

context(withRuntime: WithRuntime)
fun StorageEntry.decode(scale: String?): Any? {
    return decode(withRuntime.runtime, scale)
}

fun StorageEntry.decode(runtime: RuntimeSnapshot, scale: String?): Any? {
    val scaleOrDefault = scale ?: defaultValue()

    return scaleOrDefault?.let { type.value?.fromHex(runtime, it) }
}

fun StorageEntry.defaultValue(): String? {
    return if (modifier == StorageEntryModifier.Default) {
        default.toHexString()
    } else {
        null
    }
}

fun StorageEntry.storageKeyWith(runtime: RuntimeSnapshot, keyArguments: Array<out Any?>): String {
    return if (keyArguments.isEmpty()) {
        storageKey()
    } else {
        storageKey(runtime, *keyArguments)
    }
}

fun Module.constant(name: String) = constantOrNull(name) ?: throw NoSuchElementException("Constant $name is not found")

fun Module.constantOrNull(name: String) = constants[name]

fun Module.numberConstant(name: String, runtimeSnapshot: RuntimeSnapshot) = bindNumberConstant(constant(name), runtimeSnapshot)

context(withRuntime: WithRuntime)
fun Module.numberConstant(name: String) = bindNumberConstant(constant(name), withRuntime.runtime)

fun Module.numberConstantOrNull(name: String, runtimeSnapshot: RuntimeSnapshot) = constantOrNull(name)?.let {
    bindNumberConstant(it, runtimeSnapshot)
}

fun Constant.decoded(runtimeSnapshot: RuntimeSnapshot): Any? {
    return type?.fromByteArrayOrNull(runtimeSnapshot, value)
}

fun Module.optionalNumberConstant(name: String, runtimeSnapshot: RuntimeSnapshot) = bindNullableNumberConstant(constant(name), runtimeSnapshot)

fun RuntimeMetadata.babeOrNull() = moduleOrNull(Modules.BABE)

fun RuntimeMetadata.timestampOrNull() = moduleOrNull(Modules.TIMESTAMP)

fun RuntimeSnapshot.isParachain() = metadata.hasModule(Modules.PARACHAIN_SYSTEM)

fun RuntimeMetadata.balances() = module(Modules.BALANCES)

fun RuntimeMetadata.transactionStorage() = module(Modules.TRANSACTION_STORAGE)

fun RuntimeMetadata.privacyVoucher() = module(Modules.PRIVACY_VOUCHER)

fun RuntimeMetadata.identity() = module(Modules.IDENTITY)

fun RuntimeMetadata.resources() = module(Modules.RESOURCES)

fun RuntimeMetadata.assetRegistry() = module(ASSET_REGISTRY)

fun RuntimeMetadata.tokens() = module(TOKENS)

fun RuntimeMetadata.omnipoolOrNull() = moduleOrNull(Modules.OMNIPOOL)

fun RuntimeMetadata.omnipool() = module(Modules.OMNIPOOL)

fun RuntimeMetadata.coinage() = module(Modules.COINAGE)

fun RuntimeMetadata.pgas() = module(Modules.PGAS)

fun RuntimeMetadata.dotNsGateway() = module(Modules.DOTNS_GATEWAY)

fun RuntimeMetadata.networkSuffix() = module(Modules.NETWORK_SUFFIX)

fun RuntimeMetadata.stableSwapOrNull() = moduleOrNull(Modules.STABLE_SWAP)

fun RuntimeMetadata.xykOrNull() = moduleOrNull(Modules.XYK)

fun RuntimeMetadata.xyk() = module(Modules.XYK)

fun RuntimeMetadata.stableSwap() = module(Modules.STABLE_SWAP)

fun RuntimeMetadata.dynamicFeesOrNull() = moduleOrNull(Modules.DYNAMIC_FEES)

fun RuntimeMetadata.dynamicFees() = module(Modules.DYNAMIC_FEES)

fun RuntimeMetadata.multiTransactionPayment() = module(Modules.MULTI_TRANSACTION_PAYMENT)

fun RuntimeMetadata.xcmPalletName() = firstExistingModuleName("XcmPallet", "PolkadotXcm")

fun RuntimeMetadata.assetConversionOrNull() = moduleOrNull(Modules.ASSET_CONVERSION)

fun RuntimeMetadata.hasModule(name: String) = moduleOrNull(name) != null

fun Module.firstExistingCallName(vararg options: String): String {
    return options.first(::hasCall)
}

fun RuntimeMetadata.firstExistingModuleName(vararg options: String): String {
    return options.first(::hasModule)
}

fun Module.hasCall(name: String) = callOrNull(name) != null

fun RuntimeMetadata.hasRuntimeApisMetadata(): Boolean {
    return apis != null
}

fun RuntimeMetadata.hasDetectedRuntimeApi(section: String, method: String): Boolean {
    if (!hasRuntimeApisMetadata()) return false

    return runtimeApiOrNull(section)?.methodOrNull(method) != null
}

// View functions are only present in v16+ metadata; older runtimes expose an empty map rather than null
fun RuntimeMetadata.hasDetectedViewFunction(pallet: String, name: String): Boolean {
    return moduleOrNull(pallet)?.viewFunctionOrNull(name) != null
}

fun Any?.asRawScaleValue(): AsRawScaleValue {
    return AsRawScaleValue(this)
}

context(withRuntime: WithRuntime)
fun composeDispatchAs(
    call: GenericCall.Instance,
    origin: OriginCaller
): GenericCall.Instance {
    return composeCall(
        moduleName = Modules.UTILITY,
        callName = "dispatch_as",
        arguments = mapOf(
            "as_origin" to origin.toEncodableInstance(),
            "call" to call
        )
    )
}

context(withRuntime: WithRuntime)
fun composeBatchAll(
    calls: List<GenericCall.Instance>,
): GenericCall.Instance {
    return composeCall(
        moduleName = Modules.UTILITY,
        callName = "batch_all",
        arguments = mapOf(
            "calls" to calls
        )
    )
}

interface WithRuntime {
    val runtime: RuntimeSnapshot
}

fun WithRuntime(runtime: RuntimeSnapshot): WithRuntime {
    return InlineWithRuntime(runtime)
}

fun <T> DataType<T>.fromHex(hex: String): T {
    val codecReader = ScaleCodecReader(hex.fromHex())

    return read(codecReader)
}

fun InheritedImplication.findNonceOrThrow(): Nonce = succeedingExtensions.findExtensionOrThrow<CheckNonce>().nonce

fun InheritedImplication.findGenesisHashOrThrow(): GenesisHash =
    succeedingExtensions.findExtensionOrThrow<CheckGenesis>().genesisHash.toDataByteArray()

object Modules {
    const val MULTISIG = "Multisig"

    const val PROXY = "Proxy"

    const val ASSET_CONVERSION = "AssetConversion"

    const val UTILITY = "Utility"

    const val STABLE_SWAP = "Stableswap"

    const val XYK = "XYK"

    const val ASSET_REGISTRY = "AssetRegistry"

    const val ROUTER = "Router"

    const val OMNIPOOL = "Omnipool"

    const val DYNAMIC_FEES = "DynamicFees"

    const val MULTI_TRANSACTION_PAYMENT = "MultiTransactionPayment"

    const val TOKENS = "Tokens"

    const val ASSETS = "Assets"

    const val SYSTEM = "System"

    const val TRANSACTION_PAYMENT = "TransactionPayment"

    const val TRANSACTION_STORAGE = "TransactionStorage"

    const val PRIVACY_VOUCHER = "PrivacyVoucher"

    const val BABE = "Babe"

    const val BALANCES = "Balances"

    const val TIMESTAMP = "Timestamp"

    const val PARACHAIN_SYSTEM = "ParachainSystem"

    const val PEOPLE = "People"

    const val MEMBERS = "Members"

    const val MEMBERS_SUBSCRIBER = "MembersSubscriber"

    const val IDENTITY = "Identity"

    const val RESOURCES = "Resources"

    const val PEOPLE_LITE = "PeopleLite"

    const val COINAGE = "Coinage"

    const val PGAS = "Pgas"

    const val NETWORK_SUFFIX = "NetworkSuffix"

    const val REVIVE = "Revive"

    const val DOTNS_GATEWAY = "DotnsGateway"
}
