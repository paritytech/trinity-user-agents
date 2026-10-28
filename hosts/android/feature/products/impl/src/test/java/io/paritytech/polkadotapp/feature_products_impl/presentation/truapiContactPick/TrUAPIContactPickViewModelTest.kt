package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick

import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.awaitLoaded
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ContactPickOption
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ContactPickRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIContactPicks
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIPrompt
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.truapi.ProductId

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIContactPickViewModelTest {
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val productId = ProductId.parse("browse.dot").getOrThrow()
    private val options = listOf(ContactPickOption(byteArrayOf(1), "Alice"), ContactPickOption(byteArrayOf(2), null))

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `choosing a contact clears the rendered request before closing`() = runTest {
        val prompt = TrUAPIPrompt<ContactPickRequest, ContactPickOption?>(ContactPickRequest(productId, options), null)
        val picks = mockk<TrUAPIContactPicks> { every { current } returns prompt }
        val viewModel = TrUAPIContactPickViewModel(router, picks)

        assertEquals(
            TrUAPIContactPickUiState(productId, persistentListOf(TrUAPIContactPickRow(0, "Alice"), TrUAPIContactPickRow(1, null))),
            viewModel.state.awaitLoaded(),
        )

        viewModel.onContactClicked(1)

        assertEquals(LoadingState.Loading, viewModel.state.value)
        assertEquals(options[1], prompt.await())
        coVerify(exactly = 1) { router.closeTrUAPIContactPick() }
    }

    @Test
    fun `a restored picker without a live prompt closes without rendering a request`() = runTest {
        val answered = TrUAPIPrompt<ContactPickRequest, ContactPickOption?>(ContactPickRequest(productId, options), null)
        answered.dismiss()

        for (prompt in listOf(null, answered)) {
            val picks = mockk<TrUAPIContactPicks> { every { current } returns prompt }
            val viewModel = TrUAPIContactPickViewModel(router, picks)

            viewModel.onShown()

            assertEquals(LoadingState.Loading, viewModel.state.value)
        }
        coVerify(exactly = 2) { router.closeTrUAPIContactPick() }
    }
}
