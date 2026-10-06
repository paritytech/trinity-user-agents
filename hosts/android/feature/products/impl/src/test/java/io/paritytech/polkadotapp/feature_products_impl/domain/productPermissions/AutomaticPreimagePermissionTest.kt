package io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verifyNoInteractions
import io.paritytech.polkadotapp.test_shared.whenever
import uniffi.truapi.AuthState
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.SessionUiInfo

class AutomaticPreimagePermissionTest {
    private val oldRoot = ByteArray(32) { 1 }
    private val newRoot = ByteArray(32) { 2 }
    private val product = ProductId.fromStoredValue("product.paseo")
    private val sessions = MutableStateFlow<AuthState>(connected(oldRoot))
    private val provider = mock(TrUAPIHostRuntimeProvider::class.java)
    private val core = mock(TrUAPIHostRuntime::class.java)
    private val legacyPermissions = mock(ProductPermissionRepository::class.java)
    private val interactor = ProductPermissionsInteractor(
        mock(ProductRepository::class.java), legacyPermissions, provider,
    )
    private val rendered = AutomaticPreimagePermission(
        oldRoot, "0x" + "03".repeat(32), PermissionAuthorizationStatus.AUTHORIZED,
    )

    @Test
    fun `an old account row cannot grant revoke or reset another account`() = runTest {
        whenever(provider.sessionState).thenReturn(sessions)
        sessions.value = connected(newRoot)
        for (status in PermissionAuthorizationStatus.entries) {
            val result = runCatching { interactor.setAutomaticUploads(product, rendered, status) }
            assertTrue(result.exceptionOrNull() is IllegalStateException)
        }
        verifyNoInteractions(core, legacyPermissions)
    }

    @Test
    fun `a wallet transition disables writes before the next core session is ready`() = runTest {
        whenever(provider.sessionState).thenReturn(sessions)
        sessions.value = AuthState.Disconnected
        val result = runCatching {
            interactor.setAutomaticUploads(product, rendered, PermissionAuthorizationStatus.AUTHORIZED)
        }
        assertTrue(result.exceptionOrNull() is IllegalStateException)
        verifyNoInteractions(core, legacyPermissions)
    }

    @Test
    fun `core account mismatch rejects a stale row before any administration write`() = runTest {
        whenever(provider.sessionState).thenReturn(sessions)
        whenever(provider.runtime()).thenReturn(Result.success(core))
        whenever(core.currentSessionPublicKey()).thenReturn(newRoot)
        val result = runCatching {
            interactor.setAutomaticUploads(product, rendered, PermissionAuthorizationStatus.AUTHORIZED)
        }
        assertTrue(result.exceptionOrNull() is IllegalStateException)
        verifyNoInteractions(legacyPermissions)
        org.mockito.Mockito.verify(core).currentSessionPublicKey()
        org.mockito.Mockito.verifyNoMoreInteractions(core)
    }

    private fun connected(root: ByteArray): AuthState.Connected = AuthState.Connected(
        SessionUiInfo(root, null, null, null, null, null, null, null),
    )
}
