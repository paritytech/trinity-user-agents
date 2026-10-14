package io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.CompletableDeferred
import javax.inject.Inject
import javax.inject.Singleton

sealed interface PaymentRequestStep {
    data object Confirm : PaymentRequestStep

    data object PrivacyWarning : PaymentRequestStep
}

class PaymentRequestContext(
    val productId: ProductId,
    val amount: Balance,
    val steps: List<PaymentRequestStep>,
) {
    sealed interface Decision {
        data object Approved : Decision
        data object Rejected : Decision
    }

    private val decision = CompletableDeferred<Decision>()

    fun deliverApproved() {
        decision.complete(Decision.Approved)
    }

    fun deliverRejected() {
        decision.complete(Decision.Rejected)
    }

    suspend fun awaitDecision(): Decision = decision.await()
}

@Singleton
class PaymentRequestContextHolder @Inject constructor() {
    private var context: PaymentRequestContext? = null

    fun set(context: PaymentRequestContext) {
        this.context = context
    }

    fun get(): PaymentRequestContext? = context

    fun clear() {
        context = null
    }
}
