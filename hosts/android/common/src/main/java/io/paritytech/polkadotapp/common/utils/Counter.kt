package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update

class Counter<T> {
    private val countByKey = MutableStateFlow<Map<T, Int>>(emptyMap())

    fun increment(key: T) {
        countByKey.update { counts ->
            val count = (counts[key] ?: 0) + 1
            counts + (key to count)
        }
    }

    fun decrement(key: T) {
        countByKey.update { counts ->
            val count = counts.getValue(key) - 1
            if (count == 0) counts - key else counts + (key to count)
        }
    }

    suspend fun awaitZero(key: T) {
        countByKey.first { counts -> key !in counts }
    }
}
