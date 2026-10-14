package io.paritytech.polkadotapp.feature_coinage_api.domain.model

/** Locates one own coin or voucher key: the installation subtree it was allocated in, and its item there. */
data class CoinageKeyIndex(
    val installation: CoinageInstallationId,
    val item: Int,
)
