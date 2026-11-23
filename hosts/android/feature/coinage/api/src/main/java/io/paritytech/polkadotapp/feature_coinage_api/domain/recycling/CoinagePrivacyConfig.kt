package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

data class CoinagePrivacyConfig(
    val balanced: VoucherReadiness,
    val maxPrivacy: VoucherReadiness,
)
