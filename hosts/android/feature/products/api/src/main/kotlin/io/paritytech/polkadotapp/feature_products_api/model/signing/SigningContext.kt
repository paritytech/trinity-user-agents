package io.paritytech.polkadotapp.feature_products_api.model.signing

interface SigningContext {
    val requesterName: String
    val requesterIconUrl: String
    val signingRequestBody: SigningRequestBody

    /** The account this request signs with, resolved before the signing screen is shown. */
    val signingAccount: SigningAccount

    /**
     * Delivers what the waiting caller needs on approval. The native path signs
     * with [sign] and hands back the signature. The TrUAPI core holds the key
     * and signs after it hears yes, so its context answers without calling it.
     */
    suspend fun approve(sign: suspend () -> Result<SignedTransaction>): Result<Unit>

    suspend fun deliverRejection(): Result<Unit>

    /**
     * The sheet went away without an answer. Runs during `onCleared`, so it
     * cannot suspend, and it must be safe to call after a decision was already
     * delivered. No-op by default: a caller awaiting a signature is stranded
     * either way, but one holding a lock is not.
     */
    fun onAbandoned() = Unit
}
