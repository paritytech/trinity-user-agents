package io.paritytech.polkadotapp.feature_dotns_impl.data.contract

import io.paritytech.polkadotapp.common.domain.model.AccountId

class DotNsConfig(
    // Serves names with no registry resolver entry.
    val resolverContractAddress: AccountId,
    // Maps a node to its per-name resolver. Null disables manifest resolution.
    val registryContractAddress: AccountId?,
)
