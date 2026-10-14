package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

/**
 * The tag every durability line carries.
 *
 * Part of the contract rather than an implementation detail: a domain that exports its own logs has to
 * include this tag alongside its own, or the export shows a status changing with none of the evidence that
 * decided it. Coinage's export does.
 */
const val DURABILITY_LOG_TAG = "DurableTx"
