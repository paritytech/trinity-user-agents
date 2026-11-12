package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModelStore
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FakeFundingCore
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.entry
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.ramp
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.test.StandardTestDispatcher
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
import uniffi.truapi.FundingRail
import uniffi.truapi.ProductId

class FundingSheetViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val service = FakeFundingService()
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val interactor = mockk<FundingFlowInteractor> {
        coEvery { cash() } returns FundingFixtures.cash
        coEvery { spendable(any()) } returns null
        coEvery { brand(any()) } answers { FundingProviderBrand(firstArg(), firstArg<ProductId>().value, null) }
    }

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun sheet() = FundingSheetViewModel(
        SavedStateHandle(mapOf(FundingBottomSheet.INTENT to "intent")),
        service,
        interactor,
        router,
        testCashFormat(),
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
        coVerify { router.closeFundingOverlay("intent") }
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
    fun `backing out of the deposit before the start stops the core starting it`() = runTest(testDispatcher) {
        service.entry.emit(entry())
        val sheet = sheet()
        advanceUntilIdle()
        sheet.push(FundingScreen.DEPOSIT)

        sheet.onBack()

        assertEquals(FundingScreen.AMOUNT, sheet.screen.value?.screen)
        assertTrue("stopStarting" in service.calls)
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
        coVerify { router.closeFundingOverlay("intent") }
    }

    private fun startedOn(rail: FundingRail) = FakeFundingCore.session("intent", providerId = ramp)
        .copy(choice = FundingChoice(FakeFundingCore.quote("q-1"), rail, "EUR", null))
}
