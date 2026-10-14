package io.paritytech.polkadotapp.common.utils

import androidx.compose.runtime.Immutable
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap

interface SizedContainer {
    val size: Int
}

fun SizedContainer.isEmpty(): Boolean = size == 0

@Immutable
data class SizedList<T>(private val list: ImmutableList<T>) : ImmutableList<T> by list, SizedContainer {
    override val size: Int
        get() = list.size
}

@Immutable
data class SizedMap<K, V>(private val map: ImmutableMap<K, V>) : ImmutableMap<K, V> by map, SizedContainer {
    override val size: Int
        get() = map.size
}

fun <T> List<T>.toSizedList(): SizedList<T> = SizedList(toImmutableList())

fun <K, V> Map<K, V>.toSizedMap(): SizedMap<K, V> = SizedMap(toImmutableMap())
