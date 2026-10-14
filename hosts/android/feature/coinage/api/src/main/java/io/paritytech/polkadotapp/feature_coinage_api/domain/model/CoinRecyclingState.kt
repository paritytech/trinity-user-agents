package io.paritytech.polkadotapp.feature_coinage_api.domain.model

/**
 * What the current privacy strategy has decided about one coin.
 *
 * Derived on every evaluation rather than stored: the decision depends on how much of the balance is
 * already tied up and on the ages of every other live coin, so a verdict written once would be wrong as
 * soon as either moved.
 */
enum class CoinRecyclingState {
    /**
     * The chain will not accept this coin any longer, so it is queued for the recycler and cannot be spent
     * at all. Distinct from [TO_RECYCLE] because the user may be offered that one and never this.
     */
    MUST_RECYCLE,

    /**
     * Held back for privacy by the current strategy. Still a coin the chain would take, so a strategy may
     * offer it for spending behind a confirmation — at the cost of the privacy it was earning.
     */
    TO_RECYCLE,

    /** Free to spend. */
    ALLOW_USE
}

/**
 * Verdicts for one evaluation. A coin absent from the map has not been judged yet — it is on its way, not
 * spendable, and must never be read as available.
 */
typealias RecyclingVerdicts = Map<CoinageKeyIndex, CoinRecyclingState>
