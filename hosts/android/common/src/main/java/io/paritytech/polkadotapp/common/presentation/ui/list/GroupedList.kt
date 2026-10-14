package io.paritytech.polkadotapp.common.presentation.ui.list

typealias GroupedList<K, V> = Map<K, List<V>>

fun <K, V> emptyGroupedList() = emptyMap<K, V>()

fun <K : T, V : T, T> GroupedList<K, V>.toListWithHeaders(): List<T> = flatMap { (groupKey, values) ->
    listOf(groupKey) + values
}

inline fun <K1, V1, K2 : T, V2 : T, T : Any> GroupedList<K1, V1>.toListWithHeaders(
    keyMapper: (K1, List<V1>) -> K2?,
    valueMapper: (V1) -> V2
) = flatMap { (key, values) ->
    val mappedKey = keyMapper(key, values)
    val mappedValues = values.map(valueMapper)

    if (mappedKey != null && mappedValues.isNotEmpty()) {
        listOf(mappedKey) + mappedValues
    } else {
        mappedValues
    }
}

fun <K, V> GroupedList<K, V>.toValueList(): List<V> = flatMap { (_, values) -> values }
