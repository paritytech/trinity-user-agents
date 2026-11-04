package io.paritytech.polkadotapp.feature_wallet_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject

/** Whether this build offers adding and withdrawing CASH through a funding provider. */
class FundingEntryPoints @Inject constructor() {
    val visible: Boolean
        get() = FeatureOption.FUNDING.isEnabled
}

/**
 * [activity] where this build offers funding, and no sessions otherwise: observing the activity starts the core, which
 * a build without funding has no reason to start for it.
 */
fun FundingEntryPoints.activityOrNone(activity: () -> Flow<FundingActivity>): Flow<FundingActivity> =
    if (visible) activity() else flowOf(FundingActivity(inFlight = emptyList(), history = emptyList()))
