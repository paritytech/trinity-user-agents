package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingSheet
import kotlinx.collections.immutable.PersistentList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update

/** The funding sheets opened on top of each other, so leaving the history returns to the sheet it came from. */
class FundingSheets {
    private val stack = MutableStateFlow<PersistentList<FundingSheet>>(persistentListOf())

    val shown: Flow<FundingSheet?> = stack.map { it.lastOrNull() }

    fun open(sheet: FundingSheet) = stack.update { it.add(sheet) }

    fun back() = stack.update { if (it.isEmpty()) it else it.removeAt(it.lastIndex) }

    fun close() {
        stack.value = persistentListOf()
    }
}
