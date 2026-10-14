package io.paritytech.polkadotapp.feature_dotns_impl.data.config

import io.paritytech.polkadotapp.common.utils.HexString

internal class DotNsConfigRemote(
    val resolverContractAddress: HexString,
    // Absent in payloads published before manifest support; a null address disables manifest
    // resolution and leaves legacy resolution working.
    val registryContractAddress: HexString?,
)
