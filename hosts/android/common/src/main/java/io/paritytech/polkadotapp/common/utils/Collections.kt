package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlin.time.Duration

inline fun <reified R> List<*>.findIsInstanceOrNull(): R? {
    return find { it is R } as? R
}

fun <T> List<T>.second() = get(1)

@Suppress("UNCHECKED_CAST")
inline fun <K, V, R> Map<K, V>.mapValuesNotNull(crossinline mapper: (Map.Entry<K, V>) -> R?): Map<K, R> {
    return mapValues(mapper)
        .filterNotNull()
}

/**
 * Every key of [keys] present, with null where this map has no entry for it.
 *
 * The counterpart to [filterNotNull], for readers that must tell "no value" apart from "not asked about".
 * Batched storage reads drop keys the chain holds nothing for, which makes a complete response look
 * identical to a partial one; restoring them turns absence into a value the caller can read.
 */
fun <K, V> Map<K, V>.ensureKeysWithNullDefault(keys: Collection<K>): Map<K, V?> = keys.associateWith { this[it] }

/**
 * Every key of [keys] present, with [default] where this map has no entry for it.
 *
 * The counterpart to [ensureKeysWithNullDefault] for readers that already know what absence means — a
 * counter the chain holds no entry for is a genuine zero, not an unknown.
 */
fun <K, V : Any> Map<K, V>.ensureKeysWithDefault(keys: Collection<K>, default: V): Map<K, V> =
    keys.associateWith { this[it] ?: default }

@Suppress("UNCHECKED_CAST")
inline fun <K, V> Map<K, V?>.filterNotNull(): Map<K, V> {
    return filterValues { it != null } as Map<K, V>
}

inline fun <K, V, reified R : V> Map<K, V>.filterValuesIsInstance(): Map<K, R> {
    return mapValuesNotNull { (_, value) -> value as? R }
}

fun <T> Iterable<T>.take(n: Int, condition: (T) -> Boolean): List<T> {
    require(n >= 0) { "Requested element count $n is less than zero." }

    if (n == 0) return emptyList()

    if (this is Collection<T>) {
        if (n >= size) return filter(condition)
        if (n == 1) return listOfNotNull(first().takeIf(condition))
    }

    var count = 0
    val list = ArrayList<T>(n)
    for (item in this) {
        if (condition(item)) {
            list.add(item)
            if (++count == n)
                break
        }
    }

    return list
}

fun Iterable<Duration>.sum(): Duration = fold(Duration.ZERO) { acc, duration -> acc + duration }

fun Int.collectionIndexOrNull(): Int? {
    return takeIf { it >= 0 }
}

inline fun <T, R> Array<T>.tryFindNonNull(transform: (T) -> R?): R? {
    for (item in this) {
        val transformed = transform(item)

        if (transformed != null) return transformed
    }

    return null
}

fun Map<*, List<*>>.totalSize(): Int {
    return entries.sumOf { it.value.size }
}

fun <K, V> Map<K, V>.reversed(): Map<V, K> {
    return HashMap<V, K>().also { newMap ->
        entries.forEach { newMap[it.value] = it.key }
    }
}

fun <K, V> Map<K, V>.reversedManyToOne(): Map<V, List<K>> = entries.groupBy(
    keySelector = { it.value },
    valueTransform = { it.key }
)

fun <K1, K2, KR, V> Map<K1, Map<K2, V>>.flattenKeys(keyTransform: (K1, K2) -> KR): Map<KR, V> {
    return flatMap { (key1, innerMap) ->
        innerMap.map { (key2, value) ->
            val key = keyTransform(key1, key2)
            key to value
        }
    }.toMap()
}

inline fun <T> Iterable<T>.takeNot(n: Int, crossinline condition: (T) -> Boolean): List<T> {
    return take(n) { !condition(it) }
}

fun <T> List<T>.cycle(): Sequence<T> {
    if (isEmpty()) return emptySequence()

    var i = 0

    return generateSequence { this[i++ % this.size] }
}

inline fun <T, R> Iterable<T>.mapToSet(mapper: (T) -> R): Set<R> = mapTo(mutableSetOf(), mapper)

inline fun <T, R : Any> Iterable<T>.mapNotNullToSet(mapper: (T) -> R?): Set<R> = mapNotNullTo(mutableSetOf(), mapper)

inline fun <T, R> Iterable<T>.countDistinct(selector: (T) -> R): Int = mapToSet(selector).size

suspend fun <T, R> Iterable<T>.mapAsync(operation: suspend (T) -> R): List<R> {
    return coroutineScope {
        map { async { operation(it) } }
    }.awaitAll()
}

suspend fun <T, R> Iterable<T>.mapIndexedAsync(operation: suspend (index: Int, T) -> R): List<R> {
    return coroutineScope {
        mapIndexed { index, value -> async { operation(index, value) } }
    }.awaitAll()
}

suspend fun <T, R> Iterable<T>.mapNotNullAsync(operation: suspend (T) -> R?): List<R> {
    return mapAsync(operation).filterNotNull()
}

suspend fun <T, R> Iterable<T>.forEachAsync(operation: suspend (T) -> R) {
    mapAsync(operation)
}

suspend fun <T, R> Iterable<T>.flatMapAsync(operation: suspend (T) -> Collection<R>): List<R> {
    return mapAsync(operation).flatten()
}
