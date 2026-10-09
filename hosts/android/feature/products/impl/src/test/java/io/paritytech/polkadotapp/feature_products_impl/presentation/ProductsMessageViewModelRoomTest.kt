package io.paritytech.polkadotapp.feature_products_impl.presentation

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

private const val ROOM = "jollity"
private const val MESSAGE_TYPE = "weekly-draw"

class ProductsMessageViewModelRoomTest {
    private val testDispatcher = StandardTestDispatcher()
    private val product = Product(ProductId.fromStoredValue("dim2.dot"), "Jollity", icon = null)

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `renders against the message's own room, not one derived from the product`() = runTest(testDispatcher) {
        val worker = RecordingWorker()

        viewModelFor(worker, MESSAGE_TYPE)

        assertEquals(listOf(ProductChatIdParameter(ROOM)), worker.renderedRooms)
    }

    @Test
    fun `a ui event carries the message type the core routes by`() = runTest(testDispatcher) {
        val worker = RecordingWorker()

        viewModelFor(worker, MESSAGE_TYPE).handleUiEvent("press", JsUiEvent.Type.ButtonClick)

        assertEquals(MESSAGE_TYPE, worker.events.single().messageType)
    }

    private fun viewModelFor(worker: ProductWorker, messageType: String) = ProductsMessageViewModel(
        content = ProductsMessageContent(messageType, ByteArray(0).toDataByteArray()),
        messageId = "m1",
        worker = worker,
        product = product,
        roomId = ROOM,
        renderObservers = emptySet(),
    )
}
