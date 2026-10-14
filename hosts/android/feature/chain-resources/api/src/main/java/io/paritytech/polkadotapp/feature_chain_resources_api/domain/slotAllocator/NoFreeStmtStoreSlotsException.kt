package io.paritytech.polkadotapp.feature_chain_resources_api.domain.slotAllocator

class NoFreeStmtStoreSlotsException(val totalSlots: Int) : RuntimeException(
    "No free statement store slots available (limit = $totalSlots)"
)
