package io.paritytech.polkadotapp.feature_dotns_impl.data.repository

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld

interface NetworkSuffixRepository {
    /**
     * The runtime-wide network suffix the people chain derives product contexts from, or `null`
     * when the chain carries no value for it.
     */
    suspend fun networkSuffix(): Result<DotNsTld?>
}
