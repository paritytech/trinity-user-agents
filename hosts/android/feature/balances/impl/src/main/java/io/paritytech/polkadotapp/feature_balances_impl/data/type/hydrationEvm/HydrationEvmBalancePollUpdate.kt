package io.paritytech.polkadotapp.feature_balances_impl.data.type.hydrationEvm

import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.OrmlAssetAccount

internal class HydrationEvmBalancePollUpdate(
    val balance: OrmlAssetAccount,
    val at: BlockHash?
)
