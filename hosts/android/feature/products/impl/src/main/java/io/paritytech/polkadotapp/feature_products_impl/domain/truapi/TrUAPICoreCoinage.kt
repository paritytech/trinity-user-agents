package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.CoreCoinage
import uniffi.truapi.NativeCoinage
import javax.inject.Inject
import javax.inject.Singleton

/** Lazy because the runtime's own bridges reach the wallet, which reaches back here. */
@Singleton
class TrUAPICoreCoinage @Inject constructor(
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
) : CoreCoinage {
    override suspend fun coinage(): Result<NativeCoinage> =
        runtimeProvider.get().runtime().flatMap { runtime -> runCancellableCatching { runtime.coinage() } }
}
