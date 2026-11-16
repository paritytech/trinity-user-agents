package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingSheet
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

/** The history sheet's rows, acknowledged as the CASH card's history acknowledges them while it shows them. */
@HiltViewModel
class FundingHistoryViewModel @Inject constructor(
    fundingActivityMixinFactory: FundingActivityMixin.Factory,
    private val hostFunding: HostFunding,
) : BaseViewModel() {
    private val funding = fundingActivityMixinFactory.create(this).apply { openSheet(FundingSheet.HISTORY) }

    val activity: StateFlow<FundingActivityUiState> = funding.activity

    /** A session in flight opens where it stands, an ended one its detail, over the history. */
    fun onRowClick(id: String) = launchUnit { hostFunding.showSession(id) }
}
