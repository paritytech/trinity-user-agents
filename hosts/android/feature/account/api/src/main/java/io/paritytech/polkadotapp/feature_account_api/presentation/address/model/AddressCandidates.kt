package io.paritytech.polkadotapp.feature_account_api.presentation.address.model

import io.paritytech.polkadotapp.common.presentation.search.RemoteSearchPhase

class AddressCandidates(
    val query: String,
    val local: List<ExtractedAddressesSection>,
    val remote: RemoteSearchPhase<ExtractedAddress>,
)
