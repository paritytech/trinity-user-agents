package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import uniffi.truapi.NativeWorkerClaim

/**
 * The claims that a chat holds on the core worker of one product: one for each open room screen. Every [hold]
 * gets one release, and [close] releases what is still held when the chat stops.
 */
class WorkerHolds(
    private val claim: () -> NativeWorkerClaim,
    private val release: (NativeWorkerClaim) -> Unit,
) {
    private val lock = Any()
    private val held = ArrayDeque<NativeWorkerClaim>()
    private var closed = false
    private val openRooms = mutableMapOf<String, Int>()

    /** Throws when the core cannot give a claim, or after [close]; then nothing is held. */
    fun hold() {
        synchronized(lock) { check(!closed) { CLOSED } }
        val claimed = claim()
        val late = synchronized(lock) { closed.also { if (!it) held.addLast(claimed) } }
        if (late) {
            release(claimed)
            error(CLOSED)
        }
    }

    fun drop() {
        val claimed = synchronized(lock) { held.removeLastOrNull() }
        if (claimed != null) release(claimed)
    }

    fun openRoom(roomId: String) {
        hold()
        synchronized(lock) { openRooms[roomId] = (openRooms[roomId] ?: 0) + 1 }
    }

    /** False when the room has no open screen, so nothing was dropped. */
    fun closeRoom(roomId: String): Boolean {
        val wasOpen = synchronized(lock) {
            val count = openRooms[roomId] ?: return@synchronized false
            if (count > 1) openRooms[roomId] = count - 1 else openRooms.remove(roomId)
            true
        }
        if (wasOpen) drop()
        return wasOpen
    }

    fun isOpen(roomId: String): Boolean = synchronized(lock) { roomId in openRooms }

    /** Releases every hold and refuses each later hold, because nothing can release a hold after the chat stops. */
    fun close() {
        synchronized(lock) { closed = true }
        releaseAll()
    }

    fun releaseAll() {
        val claims = synchronized(lock) {
            openRooms.clear()
            held.toList().also { held.clear() }
        }
        claims.forEach(release)
    }

    private companion object {
        const val CLOSED = "the chat of the product is closed"
    }
}
