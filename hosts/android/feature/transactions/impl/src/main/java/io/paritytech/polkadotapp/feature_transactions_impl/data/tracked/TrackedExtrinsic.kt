package io.paritytech.polkadotapp.feature_transactions_impl.data.tracked

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ExtrinsicTag

/** A persisted tracked extrinsic the worker still has to drive to a terminal status. */
class TrackedExtrinsic(
    val tag: ExtrinsicTag,
    val chainId: ChainId,
    val signedExtrinsic: String,
)
