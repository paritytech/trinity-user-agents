package io.paritytech.polkadotapp.feature_products_api.domain.runtime

import uniffi.truapi.CoinageConfig
import uniffi.truapi.NativeCoinage

/**
 * The core's coinage layer, which holds the wallet's coins. The core runtime boots on the first call, and the
 * answer is the same until the process restarts.
 */
interface CoreCoinage {
    suspend fun coinage(): Result<NativeCoinage>
}

/**
 * How the core turns its coinage layer on: the wallet the app's own coinage engine held, handed over once, and the
 * installation registry from remote config.
 */
interface CoreCoinageSetup {
    suspend fun config(): CoinageConfig
}
