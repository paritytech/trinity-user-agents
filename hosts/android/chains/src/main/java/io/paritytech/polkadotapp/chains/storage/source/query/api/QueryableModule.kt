package io.paritytech.polkadotapp.chains.storage.source.query.api

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromByteArrayOrNull
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.novasama.substrate_sdk_android.runtime.metadata.storage
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.constant
import io.paritytech.polkadotapp.chains.util.constantOrNull
import io.paritytech.polkadotapp.chains.util.findStorage
import kotlin.reflect.KType
import kotlin.reflect.typeOf

typealias QueryableStorageToKeyBinder<K> = (keyInstance: Any?) -> K
typealias QueryableStorageFromKeyBinder<K> = (keyInstance: K) -> Any?

interface QueryableModule {
    val module: Module
}

context(withRuntime: WithRuntime)
inline fun <reified T : Any> QueryableModule.storage0(name: String): QueryableStorageEntry0<T> {
    return storage0(name, typeOf<T>())
}

context(withRuntime: WithRuntime)
inline fun <reified T : Any> QueryableModule.storage0OrNull(vararg nameCandidates: String): QueryableStorageEntry0<T>? {
    return storage0OrNull(typeOf<T>(), *nameCandidates)
}

context(withRuntime: WithRuntime)
fun <T : Any> QueryableModule.storage0(name: String, valueType: KType): QueryableStorageEntry0<T> {
    return RealQueryableStorageEntry0(module.storage(name), Entry0Encoders.Auto(valueType), withRuntime.runtime)
}

context(withRuntime: WithRuntime)
fun <T : Any> QueryableModule.storage0OrNull(valueType: KType, vararg nameCandidates: String): QueryableStorageEntry0<T>? {
    val storage = module.findStorage(*nameCandidates) ?: return null
    return RealQueryableStorageEntry0(storage, Entry0Encoders.Auto(valueType), withRuntime.runtime)
}

context(withRuntime: WithRuntime)
fun <T : Any> QueryableModule.storage0(
    name: String,
    binding: QueryableStorageBinder0<T>,
): QueryableStorageEntry0<T> {
    return RealQueryableStorageEntry0(module.storage(name), Entry0Encoders.Manual(binding), withRuntime.runtime)
}

context(withRuntime: WithRuntime)
fun <I, T> QueryableModule.storage1(
    name: String,
    binding: QueryableStorageBinder1<I, T>,
    toKeyBinding: QueryableStorageToKeyBinder<I>? = null,
    fromKeyBinding: QueryableStorageFromKeyBinder<I>? = null,
): QueryableStorageEntry1<I, T> {
    val encoders = Entry1Encoders.Manual(binding, toKeyBinding, fromKeyBinding)
    return RealQueryableStorageEntry1(withRuntime.runtime, module.storage(name), encoders)
}

context(withRuntime: WithRuntime)
inline fun <reified I, reified T> QueryableModule.storage1(name: String): QueryableStorageEntry1<I, T> {
    return storage1(name, typeOf<I>(), typeOf<T>())
}

context(withRuntime: WithRuntime)
fun <I, T> QueryableModule.storage1(name: String, keyType: KType, valueType: KType): QueryableStorageEntry1<I, T> {
    val encoders = Entry1Encoders.Auto<I, T>(keyType, valueType)
    return RealQueryableStorageEntry1(withRuntime.runtime, module.storage(name), encoders)
}

context(withRuntime: WithRuntime)
fun <I1, I2, T : Any> QueryableModule.storage2(
    name: String,
    binding: QueryableStorageBinder2<I1, I2, T>,
    toKey1Binding: QueryableStorageToKeyBinder<I1>? = null,
    toKey2Binding: QueryableStorageToKeyBinder<I2>? = null,
): QueryableStorageEntry2<I1, I2, T> {
    return RealQueryableStorageEntry2(
        runtimeSnapshot = withRuntime.runtime,
        storageEntry = module.storage(name),
        encoders = Entry2Encoders.Manual(
            binding = binding,
            toKey1Binding = toKey1Binding,
            toKey2Binding = toKey2Binding,
            fromKey1Binding = null,
            fromKey2Binding = null
        )
    )
}

context(withRuntime: WithRuntime)
inline fun <reified I1, reified I2, reified T : Any> QueryableModule.storage2(
    name: String,
): QueryableStorageEntry2<I1, I2, T> {
    return RealQueryableStorageEntry2(
        runtimeSnapshot = withRuntime.runtime,
        storageEntry = module.storage(name),
        encoders = Entry2Encoders.Auto(typeOf<I1>(), typeOf<I2>(), typeOf<T>())
    )
}

inline fun <reified I1, reified I2, reified I3, reified T : Any> QueryableModule.storage3(
    name: String
): QueryableStorageEntry3<I1, I2, I3, T> {
    return RealQueryableStorageEntry3(
        storageEntry = module.storage(name),
        key1Type = typeOf<I1>(),
        key2Type = typeOf<I2>(),
        key3Type = typeOf<I3>(),
        valueType = typeOf<T>()
    )
}

inline fun <reified I1, reified I2, reified I3, reified I4, reified T : Any> QueryableModule.storage4(
    name: String
): QueryableStorageEntry4<I1, I2, I3, I4, T> {
    return RealQueryableStorageEntry4(
        storageEntry = module.storage(name),
        key1Type = typeOf<I1>(),
        key2Type = typeOf<I2>(),
        key3Type = typeOf<I3>(),
        key4Type = typeOf<I4>(),
        valueType = typeOf<T>()
    )
}

context(withRuntime: WithRuntime)
inline fun <reified T> QueryableModule.constant(name: String): T {
    val constant = module.constant(name)
    val dynamicStructure = constant.type?.fromByteArrayOrNull(withRuntime.runtime, constant.value)

    return Scale.decode(dynamicStructure)
}

context(withRuntime: WithRuntime)
inline fun <reified T : Any> QueryableModule.constantOrNull(name: String): T? {
    val constant = module.constantOrNull(name) ?: return null
    val dynamicStructure = constant.type?.fromByteArrayOrNull(withRuntime.runtime, constant.value)

    return Scale.decode<T>(dynamicStructure)
}
