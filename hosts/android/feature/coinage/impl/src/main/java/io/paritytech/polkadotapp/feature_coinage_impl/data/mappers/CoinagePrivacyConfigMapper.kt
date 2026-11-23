package io.paritytech.polkadotapp.feature_coinage_impl.data.mappers

import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.AnonymityRequirements
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinagePrivacyConfig
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherReadiness
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinagePrivacyConfigRemote
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.PrivacyModeRemote
import kotlin.time.Duration.Companion.minutes

internal val BUNDLED_PRIVACY_CONFIG = CoinagePrivacyConfig(
    balanced = VoucherReadiness(
        requiredRingFill = 20.percents,
        anonymityRequirements = AnonymityRequirements(anonymityTime = 10.minutes, anonymityMembers = 8),
    ),
    maxPrivacy = VoucherReadiness(
        requiredRingFill = 90.percents,
        anonymityRequirements = AnonymityRequirements(anonymityTime = 10.minutes, anonymityMembers = 16),
    ),
)

internal fun CoinagePrivacyConfigRemote?.toDomain(): CoinagePrivacyConfig = CoinagePrivacyConfig(
    balanced = this?.balanced.toDomain(BUNDLED_PRIVACY_CONFIG.balanced),
    maxPrivacy = this?.maxPrivacy.toDomain(BUNDLED_PRIVACY_CONFIG.maxPrivacy),
)

private fun PrivacyModeRemote?.toDomain(bundled: VoucherReadiness): VoucherReadiness = VoucherReadiness(
    requiredRingFill = this?.ringFillPercent?.takeIf { it in 0..100 }?.percents ?: bundled.requiredRingFill,
    anonymityRequirements = bundled.anonymityRequirements?.let { bundledAnonymity ->
        AnonymityRequirements(
            anonymityTime = this?.anonymityMinutes?.takeIf { it > 0 }?.minutes ?: bundledAnonymity.anonymityTime,
            anonymityMembers = this?.anonymityMembers?.takeIf { it > 0 } ?: bundledAnonymity.anonymityMembers,
        )
    },
)
