package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import org.mockito.Mockito.mock
import uniffi.truapi.PermissionDecision

class TrUAPIConfirmationLauncherTest {
    private val holder = TrUAPIConfirmationContextHolder()
    private val router = object : ProductsRouter by mock(ProductsRouter::class.java) {
        override suspend fun openTrUAPIConfirmation() = Unit
    }
    private val launcher = TrUAPIConfirmationLauncher(holder, mock(SigningContextHolder::class.java), router)

    @Test
    fun `upload decisions preserve once automatic and deny across sheet dismissal`() = runTest {
        for (decision in PermissionDecision.entries) {
            val pending = async(start = CoroutineStart.UNDISPATCHED) { launcher.awaitDecision(upload()) }
            val context = requireNotNull(holder.get())
            when (decision) {
                PermissionDecision.ALLOW_ONCE -> context.approve()
                PermissionDecision.ALLOW_ALWAYS -> context.approveAutomatically()
                PermissionDecision.DENY -> context.reject()
            }
            // onCleared must not overwrite an explicit answer with a denial.
            context.reject()
            assertEquals(decision, pending.await())
            assertNull(holder.get())
        }
    }

    @Test
    fun `cancelled upload cannot approve the next request`() = runTest {
        val first = async(start = CoroutineStart.UNDISPATCHED) { launcher.awaitDecision(upload()) }
        val old = requireNotNull(holder.get())
        first.cancel()
        first.join()
        assertNull(holder.get())

        val next = async(start = CoroutineStart.UNDISPATCHED) { launcher.awaitDecision(upload()) }
        val current = requireNotNull(holder.get())
        old.approveAutomatically()
        holder.clear(old)
        assertSame(current, holder.get())
        assertFalse(next.isCompleted)
        current.reject()
        assertEquals(PermissionDecision.DENY, next.await())
        assertEquals(PermissionDecision.DENY, old.await())
    }

    @Test
    fun `automatic approval is unavailable for unrelated reviews`() = runTest {
        val pending = async(start = CoroutineStart.UNDISPATCHED) {
            launcher.awaitDecision(TrUAPIConfirmation.IdentityDisclosure("product.paseo"))
        }
        val context = requireNotNull(holder.get())
        context.approveAutomatically()
        assertFalse(pending.isCompleted)
        context.approve()
        assertEquals(PermissionDecision.ALLOW_ONCE, pending.await())
    }

    private fun upload() = TrUAPIConfirmation.PreimageSubmit(
        requesterProductId = "product.paseo",
        sizeBytes = 1024uL,
        rootPublicKey = "0x" + "01".repeat(32),
        genesisHash = "0x" + "02".repeat(32),
        automaticMaxBytes = 262144uL,
        automaticMaxUploads = 4u,
        automaticWindowSeconds = 3600u,
    )
}
