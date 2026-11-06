package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class ProductsMessageRendererSpinnerTest {
    private val delay = 1.seconds

    @Test
    fun `a tree that arrives before the delay shows no spinner`() = runTest {
        val state = MutableStateFlow<LoadingState<JsWidget>>(LoadingState.Loading)
        val shown = mutableListOf<Boolean>()
        val cell = launch { state.spinnerAfter(delay).collect { shown += it } }
        advanceTimeBy(delay - 1.milliseconds)
        state.value = LoadingState.Loaded(JsWidget.Spacer())
        advanceTimeBy(delay)

        assertEquals(listOf(false), shown)
        cell.cancel()
    }

    @Test
    fun `a load longer than the delay shows the spinner until the tree arrives`() = runTest {
        val state = MutableStateFlow<LoadingState<JsWidget>>(LoadingState.Loading)
        val shown = mutableListOf<Boolean>()
        val cell = launch { state.spinnerAfter(delay).collect { shown += it } }
        advanceTimeBy(delay + 1.milliseconds)
        assertEquals(listOf(false, true), shown)

        state.value = LoadingState.Loaded(JsWidget.Spacer())
        runCurrent()

        assertEquals(listOf(false, true, false), shown)
        cell.cancel()
    }
}
