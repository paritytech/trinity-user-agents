package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CheckpointBlock
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetPublicKey

/**
 * A read is three-valued. [UNKNOWN] is a read that did not answer — a transport error, a key missing from a
 * batched response, an undecodable value — and also an asset whose kind has no way to establish the fact at
 * all. It is never a verdict: a rule that needs [PRESENT] or [ABSENT] simply does not match.
 */
enum class ChainPresence { PRESENT, ABSENT, UNKNOWN }

/** What a voucher's recycler alias said, with the same three-valued reading as [ChainPresence]. */
enum class AliasRead { UNLOADED, NOT_UNLOADED, UNKNOWN }

/**
 * Everything coinage's oracle reads about the chain, gathered for one transaction against one pinned view.
 *
 * Whether a recorded block is still canonical is not here: that is true of any transaction, so the engine
 * resolves it for the whole pass at once rather than one read per transaction.
 *
 * Keys are assets' on-chain identities, and every asset of the entry appears in every map — what a read did
 * not establish is [ChainPresence.UNKNOWN] rather than a missing key, so "we did not find out" is a value
 * you can see rather than an absence you have to infer.
 *
 * Every predicate over these is written in positive form and paired with its opposite, so an unknown read
 * satisfies neither side and the rule simply does not match. Negating a read would turn a network error into
 * a verdict.
 */
data class ChainEvidence(
    val finalized: CheckpointBlock,
    val best: CheckpointBlock,
    val presenceAtFinalized: Map<AssetPublicKey, ChainPresence>,
    val presenceAtBest: Map<AssetPublicKey, ChainPresence>,
    val aliasAtFinalized: Map<AssetPublicKey, AliasRead>,
    val aliasAtBest: Map<AssetPublicKey, AliasRead>,
)
