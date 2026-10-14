package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import javax.inject.Inject

/**
 * Tells the user when a top-up ended badly.
 *
 * Raised from the operation rather than from the call that asked for it: `paymentTopUp` returns as soon as
 * the top-up is registered, so by the time there is anything to acknowledge the product has long since been
 * answered. Only the two unhappy endings are worth a prompt — a top-up that simply worked says nothing.
 */
class TopUpAcknowledgementPresenter @Inject constructor(
    private val contextHolder: TopUpRequestContextHolder,
    private val productsRouter: ProductsRouter,
) {
    suspend fun acknowledge(operation: TopUpOperation, outcome: TopUpStatus) {
        val acknowledgement = when (outcome) {
            is TopUpStatus.ClaimedPartially -> TopUpAcknowledgement.PartialPayment(
                productId = operation.productId,
                requested = operation.amount,
                credited = outcome.actualClaimed,
            )

            is TopUpStatus.NotClaimed -> TopUpAcknowledgement.Failure(operation.productId)

            else -> return
        }

        val context = TopUpRequestContext(acknowledgement)
        contextHolder.set(context)
        productsRouter.openTopUpRequestPrompt()
        context.awaitDismissed()
    }
}
