package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContext
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import kotlinx.coroutines.CompletableDeferred

/**
 * A core signing request, presented on the app's own signing sheet.
 *
 * Confirm-only, so the sheet's Sign button resolves a yes rather than producing
 * a signature: the core holds the key and signs once it hears the answer. That
 * is the only difference from [ProductSigningContext]; decoding, the details
 * modal and the layout are the native path's.
 */
class TrUAPISigningContext(
    override val requesterName: String,
    override val signingRequestBody: SigningRequestBody,
    override val signingAccount: SigningAccount,
) : SigningContext {
    // Matches the native host, which has no icon for a product either.
    override val requesterIconUrl: String = ""

    private val decision = CompletableDeferred<Boolean>()

    override suspend fun approve(sign: suspend () -> Result<SignedTransaction>): Result<Unit> {
        decision.complete(true)
        return Result.success(Unit)
    }

    /** Also the answer for a dismissed sheet, so an abandoned prompt fails closed. */
    override suspend fun deliverRejection(): Result<Unit> {
        decision.complete(false)
        return Result.success(Unit)
    }

    /**
     * A dismissed sheet has to answer, or the core waits forever and the
     * launcher's mutex holds every later confirmation behind it. A no-op once
     * a decision landed.
     */
    override fun onAbandoned() {
        decision.complete(false)
    }

    suspend fun await(): Boolean = decision.await()
}
