package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductChatExtensionTest {
    private val product = Product(id = ProductId.fromStoredValue("jollity.dot"), name = "Jollity", icon = null)

    private fun extension(worker: Result<ProductExecutable.Worker>): ProductChatExtension {
        val scriptResolver = mockk<ProductScriptResolver> {
            coEvery { resolveWorker(product.id) } returns worker
        }
        return ProductChatExtension(
            appContext = mockk(relaxed = true),
            product = product,
            workerRefCounter = mockk(relaxed = true),
            scriptResolver = scriptResolver,
        )
    }

    private fun worker(showsTextInput: Boolean) = ProductExecutable.Worker(
        scriptUrl = "https://worker.jollity.dot/index.js",
        appVersion = SemVer.ZERO,
        includesChat = true,
        includesPocket = false,
        pocketCards = emptyList(),
        showsTextInput = showsTextInput,
    )

    // The worker manifest is the product's only say over its rooms, so this is where it reaches
    // the input row.
    @Test
    fun `a worker that turns off the text input gets none`() = runTest {
        val allowed = extension(Result.success(worker(showsTextInput = false)))
            .observeUserInputAllowed(mockk<ChatId>()).first()

        assertEquals(false, allowed)
    }

    // A product that cannot be resolved keeps the text input every room had before.
    @Test
    fun `an unresolved worker keeps the text input`() = runTest {
        val allowed = extension(Result.failure(IllegalStateException("offline")))
            .observeUserInputAllowed(mockk<ChatId>()).first()

        assertEquals(true, allowed)
    }
}
