package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.CompletableDeferred
import javax.inject.Inject
import javax.inject.Singleton

/**
 * RFC-0006 top-ups claim silently — the prompt is shown only to acknowledge a problem (a failed
 * claim or a Coins amount mismatch). [acknowledgement] is what that prompt displays; the user can
 * only dismiss it, which unblocks the suspended host-API call via [awaitDismissed].
 */
class TopUpRequestContext(
    val acknowledgement: TopUpAcknowledgement,
) {
    private val dismissed = CompletableDeferred<Unit>()

    fun deliverDismissed() {
        dismissed.complete(Unit)
    }

    suspend fun awaitDismissed() = dismissed.await()
}

/** What the top-up acknowledgement prompt displays. */
sealed interface TopUpAcknowledgement {
    val productId: ProductId

    /** Nothing was claimed, and nothing more will be tried. */
    data class Failure(override val productId: ProductId) : TopUpAcknowledgement

    /** The claim succeeded but [credited] funds were accepted instead of the [requested] amount. */
    data class PartialPayment(
        override val productId: ProductId,
        val requested: Balance,
        val credited: Balance,
    ) : TopUpAcknowledgement
}

@Singleton
class TopUpRequestContextHolder @Inject constructor() {
    private var context: TopUpRequestContext? = null

    fun set(context: TopUpRequestContext) {
        this.context = context
    }

    fun get(): TopUpRequestContext? = context

    fun clear() {
        context = null
    }
}
