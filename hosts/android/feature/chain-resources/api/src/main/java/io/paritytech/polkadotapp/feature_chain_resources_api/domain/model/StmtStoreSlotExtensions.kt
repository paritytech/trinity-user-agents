package io.paritytech.polkadotapp.feature_chain_resources_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.slotAllocator.NoFreeStmtStoreSlotsException

fun List<StmtStoreSlot>.findFirstOccupiedSlotBy(accountId: AccountId): StmtStoreSlot.Occupied? =
    filterIsInstance<StmtStoreSlot.Occupied>().firstOrNull { it.accountId == accountId }

fun List<StmtStoreSlot>.findAllOccupiedIndicesBy(accountId: AccountId): List<Int> =
    mapIndexedNotNull { index, slot ->
        if (slot is StmtStoreSlot.Occupied && slot.accountId == accountId) index else null
    }

fun List<StmtStoreSlot>.findFreeSlot(): Result<Int> {
    val index = indexOfFirst { it is StmtStoreSlot.Free }
    return if (index >= 0) {
        Result.success(index)
    } else {
        Result.failure(NoFreeStmtStoreSlotsException(totalSlots = size))
    }
}
