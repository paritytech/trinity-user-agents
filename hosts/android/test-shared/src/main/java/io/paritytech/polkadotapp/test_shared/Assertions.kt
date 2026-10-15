package io.paritytech.polkadotapp.test_shared

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModelEvent
import io.paritytech.polkadotapp.common.utils.hasTheSaveValueAs
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import java.math.BigDecimal
import kotlin.time.Duration.Companion.seconds

@PublishedApi
internal val PRESENTATION_ERROR_TIMEOUT = 1.seconds

fun <T> assertListEquals(expected: List<T>, actual: List<T>, compare: (T, T) -> Boolean = { a, b -> a == b }) {
    if (expected.size != actual.size) {
        throw AssertionError("Lists are not equal. Expected: $expected, actual: $actual")
    }
    for (i in expected.indices) {
        if (!compare(expected[i], actual[i])) {
            throw AssertionError("Lists are not equal. Expected: $expected, actual: $actual")
        }
    }
}

fun assertHexEquals(expected: ByteArray, actual: ByteArray) {
    assertEquals(expected.toHexString(), actual.toHexString())
}

fun <T> assertSetEquals(expected: Set<T>, actual: Set<T>) {
    if (expected != actual) {
        throw AssertionError("Sets are not equal. Expected: $expected, actual: $actual")
    }
}

fun <K, V> assertMapEquals(expected: Map<K, V>, actual: Map<K, V>) {
    if (expected != actual) {
        throw AssertionError("Maps are not equal. Expected: $expected, actual: $actual")
    }
}

fun <V> assertAllItemsEquals(items: List<V>) {
    items.forEach {
        if (it != items[0]) {
            throw AssertionError("Items in list are not equal:\n$it\n${items[0]}")
        }
    }
}

fun assertEquals(expected: BigDecimal, actual: BigDecimal) {
    return Assert.assertTrue("Expected: $expected, got: $actual", expected hasTheSaveValueAs actual)
}

suspend inline fun <reified E> assertPresentationErrorShown(viewModel: BaseViewModel) {
    val event = withTimeout(PRESENTATION_ERROR_TIMEOUT) { viewModel.events.first() }
    val error = (event as? BaseViewModelEvent.PresentationError)?.error
    assertTrue("expected a ${E::class.simpleName} but was ${error ?: event}", error is E)
}
