package io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentError
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentService
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.WhitelistedProductsProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What a product has to get past before its payment is registered: an id it has not used, a balance that covers
 * the amount, and whichever of the two prompts apply.
 *
 * The user holds 10 private planks and 20 more that are still gaining privacy. Whether private vouchers alone
 * cover the amount is the planner's answer, stated per test.
 */
class RealRequestPaymentUseCaseTest {
    private val externalPaymentService: ExternalPaymentService = mockk()
    private val planner: ExternalPaymentPlanner = mockk()
    private val totalBalanceUseCase: TotalBalanceUseCase = mockk {
        coEvery { getBalance() } returns Result.success(balance(availablePrivate = 10, gainingPrivacy = 20))
    }
    private val permissionGuard: ProductPermissionGuard = mockk()
    private val whitelistedProductsProvider: WhitelistedProductsProvider = mockk()
    private val strategySettings: CoinageRecyclingStrategySettings = mockk()
    private val holder = PaymentRequestContextHolder()
    private val router: ProductsRouter = mockk()

    private val useCase = RealRequestPaymentUseCase(
        externalPaymentService = externalPaymentService,
        externalPaymentPlanner = planner,
        totalBalanceUseCase = totalBalanceUseCase,
        permissionGuard = permissionGuard,
        whitelistedProductsProvider = whitelistedProductsProvider,
        recyclingStrategySettings = strategySettings,
        paymentRequestContextHolder = holder,
        productsRouter = router,
    )

    private val product = ProductId.fromStoredValue("product.dot")
    private val id = ProductPaymentRequestId.fromBytes(DataByteArray(ByteArray(32) { 1 })).getOrThrow()
    private val key = ExternalPaymentKey(origin = product.value, id = id.asHex())
    private val destination = byteArrayOf(7).intoAccountId()

    private val shownSteps = mutableListOf<List<PaymentRequestStep>>()

    // ---- prompts ----

    @Test
    fun `a whitelisted product paying from private vouchers is not prompted at all`() = runBlocking<Unit> {
        givenProduct(whitelisted = true, strategy = RecyclingStrategyType.BALANCED, privateVouchersCover = true)
        givenNewPaymentRegisters()

        val result = useCase.requestPayment(product, id, planks(10), destination)

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { router.openPaymentRequestPrompt() }
        coVerify { externalPaymentService.initiatePayment(key, planks(10), destination) }
    }

    @Test
    fun `any other product asks the user to confirm the spend`() = runBlocking<Unit> {
        givenProduct(whitelisted = false, strategy = RecyclingStrategyType.MIN_PRIVACY)
        givenUserAnswers(approve = true)
        givenNewPaymentRegisters()

        val result = useCase.requestPayment(product, id, planks(10), destination)

        assertTrue(result.isSuccess)
        assertEquals(listOf(listOf(PaymentRequestStep.Confirm)), shownSteps)
    }

    /** MIN_PRIVACY holds nothing back, so there is no privacy to lose by spending more than the private part. */
    @Test
    fun `no privacy warning under the minimum privacy strategy`() = runBlocking<Unit> {
        givenProduct(whitelisted = true, strategy = RecyclingStrategyType.MIN_PRIVACY)
        givenNewPaymentRegisters()

        val result = useCase.requestPayment(product, id, planks(25), destination)

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { router.openPaymentRequestPrompt() }
    }

    @Test
    fun `spending funds still gaining privacy warns even a whitelisted product's user`() = runBlocking<Unit> {
        givenProduct(whitelisted = true, strategy = RecyclingStrategyType.BALANCED, privateVouchersCover = false)
        givenUserAnswers(approve = true)
        givenNewPaymentRegisters()

        useCase.requestPayment(product, id, planks(11), destination)

        assertEquals(listOf(listOf(PaymentRequestStep.PrivacyWarning)), shownSteps)
    }

    /**
     * The amount fits in the private balance, but part of that balance is coins: a coin loaded only to be
     * unloaded again gives up as much privacy as a voucher still gaining it.
     */
    @Test
    fun `private coins do not spare the warning`() = runBlocking<Unit> {
        givenProduct(whitelisted = true, strategy = RecyclingStrategyType.BALANCED, privateVouchersCover = false)
        givenUserAnswers(approve = true)
        givenNewPaymentRegisters()

        useCase.requestPayment(product, id, planks(10), destination)

        assertEquals(listOf(listOf(PaymentRequestStep.PrivacyWarning)), shownSteps)
    }

    /** Unlike a wallet send, a product payment may spend privacy-gaining funds under MAX_PRIVACY once warned. */
    @Test
    fun `a product payment under maximum privacy is confirmed, then warned about`() = runBlocking<Unit> {
        givenProduct(whitelisted = false, strategy = RecyclingStrategyType.MAX_PRIVACY, privateVouchersCover = false)
        givenUserAnswers(approve = true)
        givenNewPaymentRegisters()

        val result = useCase.requestPayment(product, id, planks(30), destination)

        assertTrue(result.isSuccess)
        assertEquals(listOf(listOf(PaymentRequestStep.Confirm, PaymentRequestStep.PrivacyWarning)), shownSteps)
    }

    @Test
    fun `a user who declines rejects the payment`() = runBlocking<Unit> {
        givenProduct(whitelisted = false, strategy = RecyclingStrategyType.BALANCED, privateVouchersCover = true)
        givenUserAnswers(approve = false)

        val result = useCase.requestPayment(product, id, planks(10), destination)

        assertTrue(result.exceptionOrNull() is PaymentRequestError.Rejected)
        coVerify(exactly = 0) { externalPaymentService.initiatePayment(any(), any(), any()) }
    }

    // ---- balance ----

    @Test
    fun `a product that may read the balance learns it is insufficient`() = runBlocking<Unit> {
        givenProduct(whitelisted = false, strategy = RecyclingStrategyType.BALANCED)
        coEvery { permissionGuard.check(product, ProductPermission.BalanceAccess) } returns true

        val result = useCase.requestPayment(product, id, planks(31), destination)

        assertTrue(result.exceptionOrNull() is PaymentRequestError.InsufficientBalance)
        coVerify(exactly = 0) { router.openPaymentRequestPrompt() }
    }

    /** Telling a product without BalanceAccess that the balance is the problem would leak the balance. */
    @Test
    fun `a product that may not read the balance is only told the payment was rejected`() = runBlocking<Unit> {
        givenProduct(whitelisted = false, strategy = RecyclingStrategyType.BALANCED)
        coEvery { permissionGuard.check(product, ProductPermission.BalanceAccess) } returns false

        val result = useCase.requestPayment(product, id, planks(31), destination)

        assertTrue(result.exceptionOrNull() is PaymentRequestError.Rejected)
    }

    // ---- idempotency ----

    @Test
    fun `an id the product already used is refused before the user sees anything`() = runBlocking<Unit> {
        coEvery { externalPaymentService.exists(key) } returns Result.success(true)

        val result = useCase.requestPayment(product, id, planks(10), destination)

        assertTrue(result.exceptionOrNull() is PaymentRequestError.AlreadyExists)
        coVerify(exactly = 0) { router.openPaymentRequestPrompt() }
        coVerify(exactly = 0) { externalPaymentService.initiatePayment(any(), any(), any()) }
    }

    /** Two requests under one id raced past the check; the store lets only one of them in. */
    @Test
    fun `an id taken while the user was deciding is refused`() = runBlocking<Unit> {
        givenProduct(whitelisted = true, strategy = RecyclingStrategyType.MIN_PRIVACY)
        coEvery { externalPaymentService.initiatePayment(key, any(), any()) } returns
            Result.failure(ExternalPaymentError.AlreadyExists(key))

        val result = useCase.requestPayment(product, id, planks(10), destination)

        assertTrue(result.exceptionOrNull() is PaymentRequestError.AlreadyExists)
    }

    @Test
    fun `status of a payment the product never registered is not found`() = runBlocking<Unit> {
        every { externalPaymentService.subscribePaymentStatus(key) } returns flow { throw ExternalPaymentError.NotFound(key) }

        val error = runCatching { useCase.subscribeStatus(product, id).first() }.exceptionOrNull()

        assertTrue(error is PaymentRequestError.NotFound)
    }

    private fun givenProduct(whitelisted: Boolean, strategy: RecyclingStrategyType, privateVouchersCover: Boolean = true) {
        coEvery { planner.canPayPrivately(any()) } returns Result.success(privateVouchersCover)
        coEvery { externalPaymentService.exists(key) } returns Result.success(false)
        coEvery { whitelistedProductsProvider.whitelistedProducts() } returns if (whitelisted) setOf(product) else emptySet()
        coEvery { strategySettings.getStrategy() } returns strategy
    }

    private fun givenUserAnswers(approve: Boolean) {
        coEvery { router.openPaymentRequestPrompt() } answers {
            val context = requireNotNull(holder.get())
            shownSteps += context.steps

            if (approve) context.deliverApproved() else context.deliverRejected()
        }
    }

    private fun givenNewPaymentRegisters() {
        coEvery { externalPaymentService.initiatePayment(key, any(), any()) } returns Result.success(Unit)
    }

    private fun balance(availablePrivate: Int, gainingPrivacy: Int) = CoinageBalance(
        availablePrivate = planks(availablePrivate),
        gainingPrivacy = CoinageBalance.GainingPrivacyBalance(amount = planks(gainingPrivacy), canSpendWithConfirmation = false),
        pending = planks(100),
    )

    private fun planks(value: Int): Balance = value.toBigInteger().intoBalance()
}
