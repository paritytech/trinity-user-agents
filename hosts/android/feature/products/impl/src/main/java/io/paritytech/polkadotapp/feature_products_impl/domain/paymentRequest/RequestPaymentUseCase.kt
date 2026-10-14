package io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapErrorInstance
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentError
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentService
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.WhitelistedProductsProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import javax.inject.Inject

interface RequestPaymentUseCase {
    /** Returns once the payment is registered, not once it is paid; [subscribeStatus] follows it from there. */
    suspend fun requestPayment(
        productId: ProductId,
        id: ProductPaymentRequestId,
        amount: Balance,
        destination: AccountId,
    ): Result<Unit>

    fun subscribeStatus(productId: ProductId, id: ProductPaymentRequestId): Flow<PaymentStatus>
}

/**
 * Everything the chain would accept from the user, whatever the privacy strategy is holding back: a payment the
 * user confirmed through the privacy warning may spend it all.
 */
fun CoinageBalance.spendableByProducts(): Balance = availablePrivate + gainingPrivacy.amount

class RealRequestPaymentUseCase @Inject constructor(
    private val externalPaymentService: ExternalPaymentService,
    private val externalPaymentPlanner: ExternalPaymentPlanner,
    private val totalBalanceUseCase: TotalBalanceUseCase,
    private val permissionGuard: ProductPermissionGuard,
    private val whitelistedProductsProvider: WhitelistedProductsProvider,
    private val recyclingStrategySettings: CoinageRecyclingStrategySettings,
    private val paymentRequestContextHolder: PaymentRequestContextHolder,
    private val productsRouter: ProductsRouter,
) : RequestPaymentUseCase {
    override suspend fun requestPayment(
        productId: ProductId,
        id: ProductPaymentRequestId,
        amount: Balance,
        destination: AccountId,
    ): Result<Unit> {
        val key = paymentKey(productId, id)

        return externalPaymentService.exists(key)
            .flatMap { exists ->
                if (exists) Result.failure(PaymentRequestError.AlreadyExists(id)) else totalBalanceUseCase.getBalance()
            }
            .flatMap { balance -> authorize(productId, amount, balance) }
            .flatMap { externalPaymentService.initiatePayment(key, amount, destination) }
            .mapErrorInstance<_, ExternalPaymentError.AlreadyExists> { PaymentRequestError.AlreadyExists(id) }
    }

    override fun subscribeStatus(productId: ProductId, id: ProductPaymentRequestId): Flow<PaymentStatus> =
        externalPaymentService.subscribePaymentStatus(paymentKey(productId, id))
            .catch { error -> throw if (error is ExternalPaymentError.NotFound) PaymentRequestError.NotFound(id) else error }

    private suspend fun authorize(productId: ProductId, amount: Balance, balance: CoinageBalance): Result<Unit> {
        if (balance.spendableByProducts() < amount) return Result.failure(insufficientBalanceError(productId))

        return privacyWarningNeeded(amount).flatMap { warn ->
            val steps = buildList {
                if (productId !in whitelistedProductsProvider.whitelistedProducts()) add(PaymentRequestStep.Confirm)
                if (warn) add(PaymentRequestStep.PrivacyWarning)
            }

            prompt(productId, amount, steps)
        }
    }

    private suspend fun privacyWarningNeeded(amount: Balance): Result<Boolean> {
        if (recyclingStrategySettings.getStrategy() == RecyclingStrategyType.MIN_PRIVACY) return Result.success(false)

        return externalPaymentPlanner.canPayPrivately(amount).map { !it }
    }

    private suspend fun prompt(productId: ProductId, amount: Balance, steps: List<PaymentRequestStep>): Result<Unit> {
        if (steps.isEmpty()) return Result.success(Unit)

        val context = PaymentRequestContext(productId = productId, amount = amount, steps = steps)
        paymentRequestContextHolder.set(context)
        productsRouter.openPaymentRequestPrompt()

        return when (context.awaitDecision()) {
            PaymentRequestContext.Decision.Approved -> Result.success(Unit)
            PaymentRequestContext.Decision.Rejected -> Result.failure(PaymentRequestError.Rejected())
        }
    }

    /**
     * Checked with `check`, not `requestPermission`, so the user is never prompted for BalanceAccess just to be
     * told the payment cannot happen. Without it the product learns nothing about the balance: a rejection is
     * all it gets.
     */
    private suspend fun insufficientBalanceError(productId: ProductId): PaymentRequestError =
        if (permissionGuard.check(productId, ProductPermission.BalanceAccess)) {
            PaymentRequestError.InsufficientBalance()
        } else {
            PaymentRequestError.Rejected()
        }

    private fun paymentKey(productId: ProductId, id: ProductPaymentRequestId) =
        ExternalPaymentKey(origin = productId.value, id = id.asHex())
}
