package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModelStore
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FakeFundingCore
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.entry
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.ramp
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingDepositViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingChoice
import uniffi.truapi.FundingDeposit
import uniffi.truapi.FundingProgressStep
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingStep
import uniffi.truapi.ProductId

class FundingSheetViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val service = FakeFunding()
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val interactor = mockk<FundingFlowInteractor> {
        coEvery { cashPrecision() } returns FundingFixtures.CASH_PRECISION
        coEvery { spendable() } returns Result.failure(IllegalStateException("unread"))
        coEvery { brand(any()) } answers { FundingProviderBrand(firstArg(), firstArg<ProductId>().value, null) }
    }

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun sheet() = FundingSheetViewModel(
        SavedStateHandle(mapOf(FundingBottomSheet.SESSION_ID to "intent")),
        service,
        service,
        interactor,
        router,
        testCashFormat(),
        FakeTimeProvider { 0L },
    )

    @Test
    fun `a session being set up opens on its amount`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        val sheet = sheet()
        advanceUntilIdle()

        assertEquals(FundingScreen.AMOUNT, sheet.screen.value?.screen)
    }

    @Test
    fun `a session the user comes back to opens on its deposit, as it was chosen`() = runTest(testDispatcher) {
        val choice = FundingChoice(FakeFundingCore.quote("q-1"), FundingRail.BANK, "EUR", null)
        service.entry.emit(entry(session = FakeFundingCore.session("intent", providerId = ramp).copy(choice = choice), isPresented = false))
        val sheet = sheet()
        advanceUntilIdle()

        assertEquals(FundingScreen.DEPOSIT, sheet.screen.value?.screen)
        assertEquals(FundingRail.BANK, sheet.flow.value?.rail)
        assertEquals(ramp, sheet.flow.value?.chosenProviderId)
    }

    // Restored after process death, or left behind by the core: nothing waits on the sheet any more.
    @Test
    fun `a session gone before it ended takes the sheet down`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        val sheet = sheet()
        advanceUntilIdle()

        service.entry.emit(null)
        advanceUntilIdle()

        assertTrue(sheet.isClosed)
        coVerify { router.closeFundingOverlay(FundingIntentId("intent")) }
    }

    @Test
    fun `the selection on screen is reported to the core`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        sheet()
        advanceUntilIdle()

        assertEquals("select CARD null null", service.calls.first())
    }

    @Test
    fun `a card session closes the sheet once started, a bank one shows where to send`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        val card = sheet()
        advanceUntilIdle()
        service.entry.emit(entry(session = startedOn(FundingRail.CARD), isPresented = false))
        advanceUntilIdle()
        assertTrue(card.isClosed)

        service.entry.emit(entry())
        val bank = sheet()
        advanceUntilIdle()
        service.entry.emit(entry(session = startedOn(FundingRail.BANK), isPresented = false))
        advanceUntilIdle()
        assertEquals(FundingScreen.DEPOSIT, bank.screen.value?.screen)
    }

    // A provider serving another network takes the payment in its own screen and reports no address, so the sheet
    // follows the session without one; on the app's own network the provider reports where to send.
    @Test
    fun `a crypto top-up shows its progress, and its address only once the provider reports one`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        val sheet = sheet()
        val deposit = depositOn(sheet)
        advanceUntilIdle()
        sheet.update { it.withRail(FundingRail.CRYPTO) }

        val running = startedOn(FundingRail.CRYPTO)
        service.entry.emit(entry(session = running, isPresented = false))
        advanceUntilIdle()
        assertEquals(FundingScreen.DEPOSIT, sheet.screen.value?.screen)
        assertEquals(FundingDepositContent.ProgressOnly, deposit.state.value?.content)

        service.entry.emit(entry(session = running, isPresented = false, progress = progress(deposit = cryptoDeposit)))
        advanceUntilIdle()
        assertEquals(FundingScreen.DEPOSIT, sheet.screen.value?.screen)
        assertEquals("1Address", (deposit.state.value?.content as? FundingDepositContent.Crypto)?.address)

        val received = progress(deposit = cryptoDeposit, reached = FundingStep.RECEIVED)
        service.entry.emit(entry(session = running, isPresented = false, progress = received))
        advanceUntilIdle()
        assertEquals(FundingDepositContent.ProgressOnly, deposit.state.value?.content)
    }

    @Test
    fun `a bank top-up prepares its deposit until the provider reports the account`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        val sheet = sheet()
        val deposit = depositOn(sheet)
        advanceUntilIdle()
        sheet.update { it.withRail(FundingRail.BANK) }

        val running = startedOn(FundingRail.BANK)
        service.entry.emit(entry(session = running, isPresented = false))
        advanceUntilIdle()
        assertEquals(FundingScreen.DEPOSIT, sheet.screen.value?.screen)
        assertEquals(FundingDepositContent.Waiting(preparing = true, card = false), deposit.state.value?.content)

        service.entry.emit(entry(session = running, isPresented = false, progress = progress(deposit = bankDeposit)))
        advanceUntilIdle()
        assertEquals("REF-1", (deposit.state.value?.content as? FundingDepositContent.Bank)?.reference)
    }

    @Test
    fun `a crypto session the user comes back to shows its address only if the provider reported one`() = runTest(testDispatcher) {
        service.entry.emit(entry(session = startedOn(FundingRail.CRYPTO), isPresented = false))
        val following = sheet()
        val progressOnly = depositOn(following)
        advanceUntilIdle()
        assertEquals(FundingScreen.DEPOSIT, following.screen.value?.screen)
        assertEquals(FundingDepositContent.ProgressOnly, progressOnly.state.value?.content)

        val reportedDeposit = progress(deposit = cryptoDeposit)
        service.entry.emit(entry(session = startedOn(FundingRail.CRYPTO), isPresented = false, progress = reportedDeposit))
        val reported = sheet()
        val address = depositOn(reported)
        advanceUntilIdle()
        assertEquals(FundingScreen.DEPOSIT, reported.screen.value?.screen)
        assertTrue(address.state.value?.content is FundingDepositContent.Crypto)
    }

    @Test
    fun `the provider's cancel offer opens once and closes when it is gone`() = runTest(testDispatcher) {
        val running = startedOn(FundingRail.BANK)
        service.entry.emit(entry(session = running, isPresented = false))
        val sheet = sheet()
        advanceUntilIdle()

        service.entry.emit(entry(session = running.copy(cancel = FundingCancel.Offered("1", "9", null)), isPresented = false))
        advanceUntilIdle()
        assertEquals(FundingScreen.CANCEL_OFFER, sheet.screen.value?.screen)

        service.entry.emit(entry(session = running.copy(cancel = FundingCancel.Declined), isPresented = false))
        advanceUntilIdle()
        assertEquals(FundingScreen.DEPOSIT, sheet.screen.value?.screen)
    }

    @Test
    fun `closing or clearing the sheet leaves the session`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        val sheet = sheet()
        advanceUntilIdle()

        sheet.onBack()
        advanceUntilIdle()
        ViewModelStore().apply { put("sheet", sheet) }.clear()

        assertEquals(2, service.calls.count { it == "leave" })
        coVerify { router.closeFundingOverlay(FundingIntentId("intent")) }
    }

    private fun TestScope.depositOn(sheet: FundingSheetViewModel) =
        FundingDepositViewModel(sheet, mockk(relaxed = true)).also { it.state.launchIn(backgroundScope) }

    private fun progress(deposit: FundingDeposit?, reached: FundingStep? = null) = FakeFundingCore.progress().copy(
        deposit = deposit,
        steps = listOfNotNull(reached?.let { FundingProgressStep(it, reachedAtMs = 1uL) }),
    )

    private val cryptoDeposit = FundingDeposit.Crypto(
        address = "1Address",
        network = "polkadot",
        asset = "DOT",
        amount = "10000000000",
        decimals = 10u,
        exact = true,
        uri = null,
        expiresAt = null,
    )

    private val bankDeposit = FundingDeposit.Bank(
        amount = "5000",
        currency = "EUR",
        decimals = 2u,
        beneficiary = "Ramp",
        account = "DE00",
        bankCode = null,
        reference = "REF-1",
        expiresAt = null,
    )

    private fun startedOn(rail: FundingRail) = FakeFundingCore.session("intent", providerId = ramp)
        .copy(choice = FundingChoice(FakeFundingCore.quote("q-1"), rail, "EUR", null))
}
