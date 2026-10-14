package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AutoAllowProductPermissionRequesterTest {
    private val delegate = RecordingRequester(PermissionDecision.Deny)
    private val requester = AutoAllowProductPermissionRequester(
        FakeWhitelistedProductsProvider(setOf(productId("getcash.dot"))),
        delegate,
    )

    @Test
    fun `prompt auto-allows an allow-listed product`() = runBlocking {
        val decision = requester.prompt(productId("getcash.dot"), networkAccess())

        assertEquals(PermissionDecision.AllowAlways, decision)
        assertTrue(delegate.promptCalls.isEmpty())
    }

    @Test
    fun `promptBatched auto-allows an allow-listed product`() = runBlocking {
        val decision = requester.promptBatched(
            productId("getcash.dot"),
            listOf(networkAccess(), RemotePermission.WebRtcAccess),
        )

        assertEquals(PermissionDecision.AllowAlways, decision)
        assertTrue(delegate.promptBatchedCalls.isEmpty())
    }

    @Test
    fun `prompt delegates a product that is not allow-listed`() = runBlocking {
        val decision = requester.prompt(productId("other.dot"), ProductPermission.BalanceAccess)

        assertEquals(PermissionDecision.Deny, decision)
        assertEquals(listOf(productId("other.dot")), delegate.promptCalls)
    }

    @Test
    fun `prompt delegates a subname of an allow-listed product`() = runBlocking {
        val decision = requester.prompt(productId("arena.getcash.dot"), ProductPermission.BalanceAccess)

        assertEquals(PermissionDecision.Deny, decision)
        assertEquals(listOf(productId("arena.getcash.dot")), delegate.promptCalls)
    }

    @Test
    fun `prompt delegates a bare product id carrying no root`() = runBlocking {
        val decision = requester.prompt(productId("getcash"), ProductPermission.BalanceAccess)

        assertEquals(PermissionDecision.Deny, decision)
        assertEquals(listOf(productId("getcash")), delegate.promptCalls)
    }

    private fun productId(value: String) = ProductId.fromStoredValue(value)

    private fun networkAccess() = RemotePermission.NetworkAccess("example.com")
}

private class FakeWhitelistedProductsProvider(
    private val products: Set<ProductId>,
) : WhitelistedProductsProvider {
    override suspend fun whitelistedProducts(): Set<ProductId> = products
}

private class RecordingRequester(private val decision: PermissionDecision) : ProductPermissionRequester {
    val promptCalls = mutableListOf<ProductId>()
    val promptBatchedCalls = mutableListOf<ProductId>()

    override suspend fun prompt(productId: ProductId, permission: ProductPermission): PermissionDecision {
        promptCalls += productId
        return decision
    }

    override suspend fun promptBatched(
        productId: ProductId,
        permissions: List<RemotePermission>,
    ): PermissionDecision {
        promptBatchedCalls += productId
        return decision
    }
}
