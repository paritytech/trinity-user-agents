package io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange

object SwapWeights {
    const val DEFAULT_SEGMENT_WEIGHT = 100

    object AssetConversion {
        // Asset conversion pools liquidity, they are unfavourable
        // We do x3 to allow heuristics to find routes with 3 cross-chain to be ranked even higher prioritize
        // Search via Hydration
        const val SWAP = 3 * CrossChainTransfer.TRANSFER + 10
    }

    object CrossChainTransfer {
        const val TRANSFER = DEFAULT_SEGMENT_WEIGHT
    }
}
