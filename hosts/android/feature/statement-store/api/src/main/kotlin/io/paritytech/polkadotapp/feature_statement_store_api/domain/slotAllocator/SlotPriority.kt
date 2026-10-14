package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

/**
 * Priority of a slot allocation. All comparisons go through [level]; never branch on enum
 * identity or name. Adding a new tier later (e.g. `Critical(2)`, `Background(-1)`) is a
 * one-line enum entry — filter and sort logic that uses `level` keeps working.
 *
 * Call-site mapping:
 * - [High] — SSO / auth flows that must not be silently squeezed out.
 * - [Normal] — products and host-API allocations; best-effort, willing to yield to higher tiers.
 */
enum class SlotPriority(val level: Int) {
    Normal(0),
    High(1),
}
