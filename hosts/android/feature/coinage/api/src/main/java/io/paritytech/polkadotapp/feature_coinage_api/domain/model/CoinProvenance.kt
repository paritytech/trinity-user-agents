package io.paritytech.polkadotapp.feature_coinage_api.domain.model

/**
 * Where a coin came from and what has happened to it since, which together say how private it still is.
 *
 * Written when the coin is minted and not recomputed afterwards: it records history, and history does not
 * change because the chain moved on. The exception is a coin claimed from a peer, whose age — and therefore
 * whose [hops] — is not known until the chain reports it, so [UNKNOWN] stands in until then.
 */
data class CoinProvenance(
    /**
     * The fungibility of the recycler this coin came out of, or null when that is not known — a claimed coin
     * before its presence lands, and any coin whose origin the app never observed.
     */
    val recyclerFungibility: RecyclerFungibility?,
    /** Oldest first, so `hops[0]` is the coin's first move after leaving the recycler. */
    val hops: List<Hop>,
    /**
     * How many coins arrived alongside this one when it was claimed from a peer, or null when it was not
     * claimed. Kept because [hops] cannot be built at claim time — the coin's age is not known until the
     * ownership subscription reports it — and by then the size of that arriving batch is long gone.
     */
    val incomingBundleSize: Int?,
) {
    /** Whether nothing is yet known about where this coin came from, so a later read may still fill it in. */
    val isUnobserved: Boolean = recyclerFungibility == null && hops.isEmpty()

    companion object {
        /** A coin minted straight out of a recycler, which is the only place a fungibility comes from. */
        fun fromRecycler(fungibility: RecyclerFungibility) = CoinProvenance(
            recyclerFungibility = fungibility,
            hops = emptyList(),
            incomingBundleSize = null,
        )

        /**
         * What a coin knows about itself the moment it arrives from a peer: nothing but how many arrived with
         * it. Both other fields are filled in later, from the chain's own record.
         */
        fun incoming(bundleSize: Int) = CoinProvenance(
            recyclerFungibility = null,
            hops = emptyList(),
            incomingBundleSize = bundleSize,
        )

        /** A coin whose origin the app has no record of at all — recovered by backup, or not yet observed. */
        val UNKNOWN = CoinProvenance(recyclerFungibility = null, hops = emptyList(), incomingBundleSize = null)
    }
}

/** Records a hop onto a coin's history, keeping the fungibility it inherits from the coin it came from. */
fun CoinProvenance.plusHop(hop: Hop) = copy(hops = hops + hop)
