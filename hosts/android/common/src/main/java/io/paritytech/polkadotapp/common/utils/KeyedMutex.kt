package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

// A key's Mutex is dropped once no caller holds or waits for it.
class KeyedMutex<K : Any> {
    private class Entry {
        val mutex = Mutex()
        var callers = 0
    }

    private val entries = HashMap<K, Entry>()

    suspend fun <T> withLock(key: K, action: suspend () -> T): T {
        val entry = synchronized(entries) { entries.getOrPut(key, ::Entry).also { it.callers++ } }

        try {
            return entry.mutex.withLock { action() }
        } finally {
            synchronized(entries) {
                entry.callers--
                if (entry.callers == 0) entries.remove(key)
            }
        }
    }
}
