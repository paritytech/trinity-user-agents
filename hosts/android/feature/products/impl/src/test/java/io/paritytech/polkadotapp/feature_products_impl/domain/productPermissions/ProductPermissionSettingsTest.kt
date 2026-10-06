package io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions

import io.mockk.Called
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.productSettings.ProductSettingsInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionRecord
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest

class ProductPermissionSettingsTest {
    private val productId = ProductId.fromStoredValue("calendar.dot")
    private val legacy = mockk<ProductPermissionRepository>()
    private val provider = mockk<TrUAPIHostRuntimeProvider>()
    private val runtime = mockk<TrUAPIHostRuntime>()
    private val runtimeSettings = mockk<ProductRuntimeSettings>()
    private val settings = ProductPermissionSettings(runtimeSettings, legacy, provider)
    private val bundle = PermissionAuthorizationRequest.Remote(
        RemotePermissionRequest(RemotePermission.Remote(listOf("api.example.com", "images.example.com")))
    )

    @Test
    fun `enabled settings preserve a saved bundle and toggle the exact canonical record`() = runTest {
        every { runtimeSettings.isTrUAPIRuntimeEnabled() } returns true
        coEvery { provider.constructedRuntime() } returns Result.success(runtime)
        val denied = PermissionRecord(productId.value, bundle, PermissionAuthorizationStatus.DENIED)
        val records = MutableStateFlow(listOf(denied))
        every { runtime.observePermissionRecords(productId.value) } returns records
        coEvery { runtime.setPermissionRecord(any(), any(), any()) } returns Unit
        val interactor = ProductPermissionsInteractor(mockk(), settings)

        val rows = interactor.observePermissions(productId).first()
        assertEquals(listOf(SavedProductPermission.TrUAPI(denied)), rows)
        interactor.togglePermission(productId, rows.single())
        coVerify(exactly = 1) { runtime.setPermissionRecord(productId.value, bundle, PermissionAuthorizationStatus.AUTHORIZED) }

        records.value = listOf(denied.copy(status = PermissionAuthorizationStatus.AUTHORIZED))
        interactor.togglePermission(productId, interactor.observePermissions(productId).first().single())
        coVerify(exactly = 1) { runtime.setPermissionRecord(productId.value, bundle, PermissionAuthorizationStatus.DENIED) }
        verify { legacy wasNot Called }
        coVerify(exactly = 0) { provider.runtime() }
    }

    @Test
    fun `account settings preserve the projected caller and canonical target`() = runTest {
        every { runtimeSettings.isTrUAPIRuntimeEnabled() } returns true
        coEvery { provider.constructedRuntime() } returns Result.success(runtime)
        val request = PermissionAuthorizationRequest.AccountAccess("mail")
        val record = PermissionRecord(productId.value, request, PermissionAuthorizationStatus.AUTHORIZED)
        every { runtime.observePermissionRecords(productId.value) } returns flowOf(listOf(record))
        coEvery { runtime.setPermissionRecord(any(), any(), any()) } returns Unit
        val interactor = ProductPermissionsInteractor(mockk(), settings)

        interactor.togglePermission(productId, interactor.observePermissions(productId).first().single())

        coVerify(exactly = 1) { runtime.setPermissionRecord(productId.value, request, PermissionAuthorizationStatus.DENIED) }
        coVerify(exactly = 0) { runtime.setPermissionRecord("calendar", any(), any()) }
        verify { legacy wasNot Called }
    }

    @Test
    fun `disabled settings retain Room grants and denial without constructing Rust`() = runTest {
        every { runtimeSettings.isTrUAPIRuntimeEnabled() } returns false
        val denied = ProductPermissionStatus(ProductPermission.UserIdentityAccess, granted = false)
        every { legacy.observeAllByProduct(productId) } returns flowOf(listOf(denied))
        coEvery { legacy.grant(productId, denied.permission) } returns Unit
        coEvery { legacy.revoke(productId, denied.permission) } returns Unit
        val interactor = ProductPermissionsInteractor(mockk(), settings)

        assertEquals(listOf(SavedProductPermission.Legacy(denied)), interactor.observePermissions(productId).first())
        interactor.togglePermission(productId, SavedProductPermission.Legacy(denied))
        interactor.togglePermission(productId, SavedProductPermission.Legacy(denied.copy(granted = true)))

        coVerify(exactly = 1) { legacy.grant(productId, denied.permission) }
        coVerify(exactly = 1) { legacy.revoke(productId, denied.permission) }
        verify { provider wasNot Called }
    }

    @Test
    fun `product settings visibility includes saved denials and follows their removal`() = runTest {
        every { runtimeSettings.isTrUAPIRuntimeEnabled() } returns true
        coEvery { provider.constructedRuntime() } returns Result.success(runtime)
        val records = MutableStateFlow(listOf(PermissionRecord(productId.value, bundle, PermissionAuthorizationStatus.DENIED)))
        every { runtime.observePermissionRecords(productId.value) } returns records
        val product = mockk<Product> {
            every { id } returns productId
            every { icon } returns null
        }
        val catalog = mockk<ProductRepository> {
            every { observeProducts() } returns flowOf(listOf(product))
        }
        val interactor = ProductSettingsInteractor(catalog, settings, mockk())

        assertTrue(interactor.observeProductSettings(productId).first()!!.hasPermissions)
        records.value = emptyList()
        assertFalse(interactor.observeProductSettings(productId).first()!!.hasPermissions)
        verify { legacy wasNot Called }
    }

    @Test
    fun `construction observer and update failures stay failures without falling back to Room`() = runTest {
        every { runtimeSettings.isTrUAPIRuntimeEnabled() } returns true
        val failure = IllegalStateException("Protected records unavailable")
        coEvery { provider.constructedRuntime() } returns Result.failure(failure)
        assertSame(failure, runCatching { settings.observePermissions(productId).first() }.exceptionOrNull())

        coEvery { provider.constructedRuntime() } returns Result.success(runtime)
        every { runtime.observePermissionRecords(productId.value) } returns flow { throw failure }
        assertSame(failure, runCatching { settings.observePermissions(productId).first() }.exceptionOrNull())

        coEvery { runtime.setPermissionRecord(any(), any(), any()) } throws failure
        val record = PermissionRecord(productId.value, bundle, PermissionAuthorizationStatus.DENIED)
        assertSame(failure, runCatching { settings.togglePermission(productId, SavedProductPermission.TrUAPI(record)) }.exceptionOrNull())
        verify { legacy wasNot Called }
    }
}
