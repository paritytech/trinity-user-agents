package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import android.content.Context
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

private const val DESCRIPTION = "Play the weekly game with friends"

class ProductsMessageRendererDescriptionTest {
    private val context: Context = mockk {
        every { getString(RCommon.string.products_message_custom_preview) } returns "Custom message"
    }
    private val product = Product(ProductId.fromStoredValue("dim2.dot"), "Jollity", icon = null)

    // The sender describes each card, so the line says what this one shows rather than what the product is.
    @Test
    fun `a custom message is described by the alt the product gave it`() = runTest {
        assertEquals(Result.success("Weekly draw"), describe(custom(alt = "Weekly draw"), description = DESCRIPTION))
    }

    @Test
    fun `a custom message without an alt is described by the product's manifest description`() = runTest {
        assertEquals(Result.success(DESCRIPTION), describe(custom(alt = null), description = DESCRIPTION))
    }

    // A product with no manifest has no description.
    @Test
    fun `a custom message of a product without a description is described by the generic label`() = runTest {
        assertEquals(Result.success("Custom message"), describe(custom(alt = null), description = null))
    }

    private suspend fun describe(message: ChatMessage.Content.Custom<ProductsMessageContent>, description: String?): Result<String> {
        val resolveProduct = mockk<ResolveProductUseCase> {
            coEvery { resolve(product.id) } returns Result.success(
                ResolvedProduct(product, description, Executables(app = null, widget = null, worker = null))
            )
        }

        return ProductsMessageRenderer(context, product, RecordingWorker(), mockk(), resolveProduct).formatNotificationContent(message)
    }

    private fun custom(alt: String?) = ChatMessage.Content.Custom(
        rendererId = product.id.toChatExtensionId(),
        content = Result.success(ProductsMessageContent("weekly-draw", ByteArray(0).toDataByteArray(), alt)),
    )
}
