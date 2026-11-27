package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingEntriesUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingSheet
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow

interface FundingEntriesMixin {
    val activity: StateFlow<FundingEntriesUiState>

    val sheet: StateFlow<FundingSheet?>

    /** Get or Send to yourself: the sessions in progress that way show first, so a second one is a deliberate choice. */
    fun onStart(direction: FundingDirection)

    fun onStartNew(direction: FundingDirection)

    /** Opens the history over the sheet on screen. */
    fun onHistory()

    /** The history is on screen as a screen of its own: its ended sessions are acknowledged for as long as the mixin's scope lives. */
    fun onHistoryShown()

    fun onSheetBack()

    fun onSheetDismiss()

    /** A session in flight opens where it stands; an ended one opens its detail. */
    fun onRowClick(id: String)

    interface Factory {
        /** [onFailure] shows why funding could not open, including when no provider is listed. */
        fun create(scope: CoroutineScope, onFailure: (PresentationThrowable) -> Unit): FundingEntriesMixin
    }
}
