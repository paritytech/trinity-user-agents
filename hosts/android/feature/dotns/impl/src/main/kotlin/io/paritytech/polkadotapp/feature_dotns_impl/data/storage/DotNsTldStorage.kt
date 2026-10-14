package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld

interface DotNsTldStorage {
    fun getTld(): DotNsTld?

    fun putTld(tld: DotNsTld)
}
