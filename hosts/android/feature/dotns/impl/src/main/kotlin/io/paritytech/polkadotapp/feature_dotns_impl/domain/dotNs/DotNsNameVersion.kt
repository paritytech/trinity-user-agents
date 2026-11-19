package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import kotlinx.serialization.Serializable

@Serializable
data class DotNsNameVersion(val contentHash: ContentHash?, val records: Map<String, String?>)
