package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun

import io.paritytech.polkadotapp.chains.network.binding.Balance

internal class XcmTransferDryRunResult(
    val origin: IntermediateSegment,
    val remoteReserve: IntermediateSegment?,
    val destination: FinalSegment,
) {
    class IntermediateSegment(
        val deliveryFee: Balance,
        val trapped: Balance,
    )

    class FinalSegment(
        val depositedAmount: Balance
    )
}
