package io.paritytech.polkadotapp.feature_coinage_impl.data.config

import androidx.annotation.Keep

@Keep
internal class CoinagePrivacyConfigRemote(
    val balanced: PrivacyModeRemote?,
    val maxPrivacy: PrivacyModeRemote?,
)

@Keep
internal class PrivacyModeRemote(
    val ringFillPercent: Int?,
    val anonymityMinutes: Int?,
    val anonymityMembers: Int?,
)
