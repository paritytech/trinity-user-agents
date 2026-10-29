package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

/**
 * Priority of a slot allocation. All comparisons go through [level] and [isEvictable]; never
 * branch on enum identity or name.
 *
 * Call-site mapping:
 * - [Critical] — per-chat anonymous signers. Never evicted, not even by another [Critical]
 *   caller, so a private chat cannot be silently downgraded; may evict [High] and [Normal].
 * - [High] — SSO / auth flows that must not be silently squeezed out.
 * - [Normal] — products and host-API allocations; best-effort, willing to yield to higher tiers.
 */
enum class SlotPriority(val level: Int, val isEvictable: Boolean) {
    Normal(0, isEvictable = true),
    High(1, isEvictable = true),
    Critical(2, isEvictable = false),
}

fun SlotPriority.canBeEvictedBy(caller: SlotPriority): Boolean {
    return isEvictable && level <= caller.level
}
