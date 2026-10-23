package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import android.content.Context
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

class ProductsMessageRendererDescriptionTest {
    private val context: Context = mockk {
        every { getString(RCommon.string.products_message_custom_preview) } returns "Custom message"
    }
    private val product = Product(ProductId.fromStoredValue("dim2.dot"), "Jollity", icon = null)
    private val renderer = ProductsMessageRenderer(context, product, RecordingWorker(), mockk())

    @Test
    fun `a custom message is described by the alt the product gave it`() = runTest {
        assertEquals(Result.success("Weekly draw"), renderer.formatNotificationContent(custom(alt = "Weekly draw")))
    }

    @Test
    fun `a custom message without an alt is described by the generic label`() = runTest {
        assertEquals(Result.success("Custom message"), renderer.formatNotificationContent(custom(alt = null)))
    }

    private fun custom(alt: String?) = ChatMessage.Content.Custom(
        rendererId = renderer.id,
        content = Result.success(ProductsMessageContent("weekly-draw", ByteArray(0).toDataByteArray(), alt)),
    )
}
