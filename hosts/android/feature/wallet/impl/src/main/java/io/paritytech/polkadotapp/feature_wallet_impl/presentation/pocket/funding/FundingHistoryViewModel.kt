package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

@HiltViewModel
class FundingHistoryViewModel @Inject constructor(
    fundingActivityMixinFactory: FundingActivityMixin.Factory,
    private val hostFunding: HostFunding,
) : BaseViewModel() {
    private val funding = fundingActivityMixinFactory.create(this) { showPresentationError(it) }.apply { onHistory() }

    val activity: StateFlow<FundingActivityUiState> = funding.activity

    // Unlike the card's rows, the history stays under the session it opens.
    fun onRowClick(id: String) = launchUnit { hostFunding.showSession(id) }
}
