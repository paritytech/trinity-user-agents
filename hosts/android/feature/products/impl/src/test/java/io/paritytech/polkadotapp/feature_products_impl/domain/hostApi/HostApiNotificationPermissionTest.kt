package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.RealProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.DeviceCapabilityPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionDeniedException
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions

class HostApiNotificationPermissionTest {
    private val requester: RealProductPermissionRequester = mock()
    private val osHandler: DeviceCapabilityPermissionHandler = mock()
    private val interactor = HostApiInteractor(
        deriveEntropyUseCase = mock(),
        productAccountDerivationUseCase = mock(),
        usernameOfAccountUseCase = mock(),
        chainRegistry = mock(),
        connectionSecrets = mock(),
        permissionGuard = mock(),
        permissionRequester = requester,
        deviceCapabilityPermissionHandler = osHandler,
        ipfsContentLookup = mock(),
        statementStoreService = mock(),
        statementStoreMessageProverFactory = mock(),
        productNotificationPublisher = mock(),
        productNotificationScheduler = mock(),
        totalBalanceUseCase = mock(),
        requestPaymentUseCase = mock(),
        topUpService = mock(),
        signedOrigins = mock(),
        accountsProtocol = mock(),
        allowanceKeyUseCase = mock(),
        transactionStorageService = mock(),
        preimageSubmitSponsoring = mock(),
        statementStoreSubmissionSponsoring = mock(),
        appThemeSelector = mock(),
        productRequestAccountResolver = mock(),
        productSigningScreenLauncher = mock(),
    )

    // The classifier is the real core export, loaded by the module's existing
    // host-cdylib test setup. Only the OS boundary and application UI are faked.
    @Test
    fun `blessed notifications skip app consent across networks but request OS permission`() = runTest {
        whenever(osHandler.requestOsPermissionIfNeeded(DeviceCapabilityType.Notifications)).thenReturn(true)
        for (label in listOf("peopl", "dim2", "stash")) {
            for (network in listOf("dot", "paseo", "testnet")) {
                val result = interactor.requestDevicePermissionDecision(
                    ProductId.fromStoredValue("$label.$network"),
                    DeviceCapabilityType.Notifications,
                )
                assertEquals(PermissionDecision.AllowAlways, result.getOrThrow())
            }
        }
        verifyNoInteractions(requester)
        verify(osHandler, org.mockito.Mockito.times(9))
            .requestOsPermissionIfNeeded(DeviceCapabilityType.Notifications)
    }

    @Test
    fun `OS refusal prevents blessed notification authorization`() = runTest {
        whenever(osHandler.requestOsPermissionIfNeeded(DeviceCapabilityType.Notifications)).thenReturn(false)

        val result = interactor.requestDevicePermissionDecision(
            ProductId.fromStoredValue("peopl.dot"), DeviceCapabilityType.Notifications,
        )

        assertTrue(result.exceptionOrNull() is ProductPermissionDeniedException)
        verifyNoInteractions(requester)
        verify(osHandler).requestOsPermissionIfNeeded(DeviceCapabilityType.Notifications)
    }

    @Test
    fun `ordinary notification refusal stops before OS authorization`() = runTest {
        val product = ProductId.fromStoredValue("app.peopl.dot")
        val permission = ProductPermission.DeviceCapability(DeviceCapabilityType.Notifications)
        whenever(requester.prompt(product, permission)).thenReturn(PermissionDecision.Deny)

        val result = interactor.requestDevicePermissionDecision(product, DeviceCapabilityType.Notifications)

        assertEquals(PermissionDecision.Deny, result.getOrThrow())
        verify(requester).prompt(product, permission)
        verifyNoInteractions(osHandler)
    }

    @Test
    fun `blessed camera and microphone still require app consent`() = runTest {
        val product = ProductId.fromStoredValue("peopl.dot")
        for (capability in listOf(DeviceCapabilityType.Camera, DeviceCapabilityType.Microphone)) {
            val permission = ProductPermission.DeviceCapability(capability)
            whenever(requester.prompt(product, permission)).thenReturn(PermissionDecision.Deny)

            assertEquals(
                PermissionDecision.Deny,
                interactor.requestDevicePermissionDecision(product, capability).getOrThrow(),
            )
            verify(requester).prompt(product, permission)
        }
        verifyNoInteractions(osHandler)
    }

    @Test
    fun `ordinary notification allow once retains its lifetime after OS authorization`() = runTest {
        val product = ProductId.fromStoredValue("ordinary.dot")
        val permission = ProductPermission.DeviceCapability(DeviceCapabilityType.Notifications)
        whenever(requester.prompt(product, permission)).thenReturn(PermissionDecision.AllowOnce)
        whenever(osHandler.requestOsPermissionIfNeeded(DeviceCapabilityType.Notifications)).thenReturn(true)

        val result = interactor.requestDevicePermissionDecision(product, DeviceCapabilityType.Notifications)

        assertEquals(PermissionDecision.AllowOnce, result.getOrThrow())
        verify(requester).prompt(product, permission)
        verify(osHandler).requestOsPermissionIfNeeded(DeviceCapabilityType.Notifications)
    }
}
