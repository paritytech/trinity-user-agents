package io.paritytech.polkadotapp.feature_wallet_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import javax.inject.Inject

/** Whether this build offers adding and withdrawing CASH through a funding provider. */
class FundingEntryPoints @Inject constructor() {
    val visible: Boolean
        get() = FeatureOption.FUNDING.isEnabled
}
