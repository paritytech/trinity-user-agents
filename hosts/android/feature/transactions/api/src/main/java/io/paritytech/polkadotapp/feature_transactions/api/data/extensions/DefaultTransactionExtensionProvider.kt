package io.paritytech.polkadotapp.feature_transactions.api.data.extensions

import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

/**
 * Contributes a [TransactionExtension] to every extrinsic submitted on a matching chain.
 *
 * Implementations are bound into a `Set<DefaultTransactionExtensionProvider>` via Dagger
 * `@IntoSet`; `ExtrinsicService` iterates the set on each submission and applies every
 * non-null result returned by [provideFor]. Use this for cross-feature, per-chain,
 * always-on transaction extensions that no individual caller should have to remember.
 */
interface DefaultTransactionExtensionProvider {
    /** Returns a [TransactionExtension] to add to every extrinsic on [chainId], or `null` if not applicable. */
    fun provideFor(chainId: ChainId): TransactionExtension?
}
