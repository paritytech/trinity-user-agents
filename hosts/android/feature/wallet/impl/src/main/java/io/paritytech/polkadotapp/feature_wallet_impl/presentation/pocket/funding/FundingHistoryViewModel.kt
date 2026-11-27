package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingEntriesUiState
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

@HiltViewModel
class FundingHistoryViewModel @Inject constructor(
    fundingEntriesMixinFactory: FundingEntriesMixin.Factory,
) : BaseViewModel() {
    private val funding = fundingEntriesMixinFactory.create(this) { showPresentationError(it) }.apply { onHistoryShown() }

    val activity: StateFlow<FundingEntriesUiState> = funding.activity

    // This history is no sheet of the mixin's, so it stays under the session it opens.
    fun onRowClick(id: String) = funding.onRowClick(id)
}
