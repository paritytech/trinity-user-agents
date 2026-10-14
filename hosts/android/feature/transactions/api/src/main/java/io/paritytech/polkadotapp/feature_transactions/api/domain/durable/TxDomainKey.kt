package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

import dagger.MapKey

/**
 * Keys a domain's [TxCompletionOracle] into the engine's map of them.
 *
 * A `String` rather than [TxDomainId] because Dagger map keys must be constant expressions; the engine
 * wraps it back on the way in. A domain that binds no oracle is decided by the block search alone, which
 * is correct — just slower.
 */
@MapKey
annotation class TxDomainKey(val value: String)
