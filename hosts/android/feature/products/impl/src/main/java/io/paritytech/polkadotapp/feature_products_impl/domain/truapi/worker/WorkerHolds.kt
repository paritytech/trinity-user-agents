package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

/**
 * The references that a chat holds on the core worker of one product (02-DESIGN §5.2, D10): one for each
 * open room screen, and short ones without a room. Every [hold] gets one release, and [releaseAll] gives
 * back what is still held when the chat goes away.
 */
class WorkerHolds(
    private val acquire: () -> Unit,
    private val release: () -> Unit,
) {
    private val lock = Any()
    private var held = 0
    private val openRooms = mutableMapOf<String, Int>()

    /** Throws when the core cannot take the reference; then nothing is held. */
    fun hold() {
        acquire()
        synchronized(lock) { held++ }
    }

    fun drop() {
        val holding = synchronized(lock) { (held > 0).also { if (it) held-- } }
        if (holding) release()
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

    fun releaseAll() {
        val count = synchronized(lock) {
            openRooms.clear()
            held.also { held = 0 }
        }
        repeat(count) { release() }
    }
}
