package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.ProductPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.suspendCancellableCoroutine
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.resume
import kotlin.math.max

class RealProductPermissionGuardTest {
    private val remoteHandler: ProductPermissionHandler<RemotePermission> = mock()
    private val accountHandler: ProductPermissionHandler<ProductPermission.AccountAccess> = mock()
    private val balanceHandler: ProductPermissionHandler<ProductPermission.BalanceAccess> = mock()
    private val deviceHandler: ProductPermissionHandler<ProductPermission.DeviceCapability> = mock()
    private val identityHandler: ProductPermissionHandler<ProductPermission.UserIdentityAccess> = mock()
    private val repository: ProductPermissionRepository = mock()
    private val requester: ProductPermissionRequester = mock()

    private val productId = ProductId.fromStoredValue("acme.dot")
    private val permission = RemotePermission.WebRtcAccess

    private lateinit var guard: RealProductPermissionGuard

    @Before
    fun setUp() {
        guard = guardWith(remoteHandler, requester)
    }

    @Test
    fun `requestPermission returns true without prompting when already granted`() = runBlocking<Unit> {
        withAlreadyGranted()

        val result = guard.requestPermission(productId, permission)

        assertTrue(result)
        verifyNoHandlerRequest()
    }

    @Test
    fun `requestPermission prompts handler when not granted`() = runBlocking<Unit> {
        withNotGranted()
        withHandlerGranting()

        val result = guard.requestPermission(productId, permission)

        assertTrue(result)
        verifyHandlerRequested()
    }

    @Test
    fun `requestPermission skips handler when permission is granted while waiting for the lock`() = runBlocking<Unit> {
        withGrantedWhileWaitingForLock()

        val result = guard.requestPermission(productId, permission)

        assertTrue(result)
        verifyNoHandlerRequest()
    }

    @Test
    fun `requestPermissionsBatched skips prompt when permissions are granted while waiting for the lock`() = runBlocking<Unit> {
        withGrantedWhileWaitingForLock()

        val result = guard.requestPermissionsBatched(productId, listOf(permission))

        assertTrue(result)
        verifyNoBatchedPrompt()
    }

    @Test
    fun `requestPermissionsBatched grants permanently when decision is AllowAlways`() = runBlocking<Unit> {
        withNotGranted()
        withBatchedDecision(PermissionDecision.AllowAlways)

        val result = guard.requestPermissionsBatched(productId, listOf(permission))

        assertTrue(result)
        verifyGrantedPermanently()
    }

    @Test
    fun `consumePermission consumes a grant issued while waiting for the lock without prompting`() = runBlocking<Unit> {
        withOneTimeGrantIssuedWhileWaiting()

        val result = guard.consumePermission(productId, permission)

        assertTrue(result)
        verifyNoHandlerRequest()
    }

    @Test
    fun `consumePermission returns true from an existing one-time grant without prompting`() = runBlocking<Unit> {
        withExistingOneTimeGrant()

        val result = guard.consumePermission(productId, permission)

        assertTrue(result)
        verifyNoHandlerRequest()
    }

    @Test
    fun `only one request enters the lock at a time when calls are concurrent`() = runBlocking<Unit> {
        val coordinator = PromptCoordinator()
        val serializedGuard = guardWith(CoordinatedHandler(coordinator), CoordinatedRequester(coordinator))
        withNoExistingGrant()

        // Mix both entry points to prove they serialize against each other, not just within a single method
        val jobs = listOf(
            launch { serializedGuard.requestPermission(productId, RemotePermission.WebRtcAccess) },
            launch { serializedGuard.requestPermissionsBatched(productId, listOf(RemotePermission.NetworkAccess("a.example"))) },
            launch { serializedGuard.requestPermission(productId, RemotePermission.ChainSubmitAccess) },
            launch { serializedGuard.requestPermissionsBatched(productId, listOf(RemotePermission.NetworkAccess("b.example"))) },
            launch { serializedGuard.requestPermission(productId, RemotePermission.StatementSubmitAccess) },
        )

        // Release parked requests one at a time; if the lock let two in at once, maxConcurrent (a running peak) records it
        repeat(jobs.size) {
            coordinator.awaitParkedRequest().resume(PermissionDecision.AllowAlways)
        }
        jobs.joinAll()

        assertEquals(jobs.size, coordinator.totalRequests.get())
        assertEquals(1, coordinator.maxConcurrent.get())
    }

    @Test
    fun `check completes while a request is parked holding the lock`() = runBlocking<Unit> {
        val coordinator = PromptCoordinator()
        val lockedGuard = guardWith(CoordinatedHandler(coordinator), CoordinatedRequester(coordinator))
        withNoExistingGrant()

        val requestJob = launch { lockedGuard.requestPermission(productId, RemotePermission.WebRtcAccess) }
        val pendingRequest = coordinator.awaitParkedRequest() // request now parked, holding the mutex

        // check touches no lock, so it must complete while the request stays parked under the mutex
        val checkResult = lockedGuard.check(productId, RemotePermission.ChainSubmitAccess)
        assertFalse(checkResult)

        pendingRequest.resume(PermissionDecision.Deny)
        requestJob.join()
    }

    // region setup helpers

    private fun withNoExistingGrant() {
        whenever(repository.hasOneTimeGrant(any(), any())).thenReturn(false)
    }

    private fun withAlreadyGranted() {
        whenever(repository.hasOneTimeGrant(any(), any())).thenReturn(true)
    }

    private suspend fun withNotGranted() {
        withNoExistingGrant()
        whenever(remoteHandler.isGranted(any(), any())).thenReturn(false)
    }

    private suspend fun withHandlerGranting() {
        whenever(remoteHandler.request(any(), any())).thenReturn(true)
    }

    /** Pre-lock check sees the permission ungranted; the under-lock re-check sees it granted. */
    private suspend fun withGrantedWhileWaitingForLock() {
        whenever(repository.hasOneTimeGrant(any(), any())).thenReturn(false, true)
        whenever(remoteHandler.isGranted(any(), any())).thenReturn(false)
    }

    private fun withExistingOneTimeGrant() {
        whenever(repository.consumeOneTimeGrant(any(), any())).thenReturn(true)
    }

    /** First consume attempt (pre-lock) finds nothing; the under-lock retry finds a freshly issued grant. */
    private fun withOneTimeGrantIssuedWhileWaiting() {
        whenever(repository.consumeOneTimeGrant(any(), any())).thenReturn(false, true)
    }

    private suspend fun withBatchedDecision(decision: PermissionDecision) {
        whenever(requester.promptBatched(any(), any())).thenReturn(decision)
    }

    // endregion

    // region verify helpers

    private suspend fun verifyHandlerRequested() {
        verify(remoteHandler).request(any(), any())
    }

    private suspend fun verifyNoHandlerRequest() {
        verify(remoteHandler, never()).request(any(), any())
    }

    private suspend fun verifyNoBatchedPrompt() {
        verify(requester, never()).promptBatched(any(), any())
    }

    private suspend fun verifyGrantedPermanently() {
        verify(repository).grant(any(), any())
    }

    // endregion

    private fun guardWith(
        remotePermissionHandler: ProductPermissionHandler<RemotePermission>,
        requester: ProductPermissionRequester,
    ) = RealProductPermissionGuard(
        remotePermissionHandler,
        accountHandler,
        balanceHandler,
        deviceHandler,
        identityHandler,
        repository,
        requester,
    )

    /**
     * A fake handler/requester backing store: it parks every request/prompt on a stored continuation instead of
     * resolving it, so a test can observe how many requests are concurrently inside the lock and release them one at a
     * time. Mockito can't suspend a stubbed `suspend` call, so a fake is the only way to control the suspension point
     * deterministically — no sleep-based timing windows.
     */
    private class PromptCoordinator {
        private val parked = Channel<CancellableContinuation<PermissionDecision>>(Channel.UNLIMITED)
        private val inFlight = AtomicInteger(0)

        val maxConcurrent = AtomicInteger(0)
        val totalRequests = AtomicInteger(0)

        /** Suspends until a request is parked, then hands back its continuation for the test to resume. */
        suspend fun awaitParkedRequest(): CancellableContinuation<PermissionDecision> = parked.receive()

        suspend fun park(): PermissionDecision {
            totalRequests.incrementAndGet()
            val active = inFlight.incrementAndGet()
            maxConcurrent.getAndUpdate { observed -> max(observed, active) }
            try {
                return suspendCancellableCoroutine { continuation -> parked.trySend(continuation) }
            } finally {
                inFlight.decrementAndGet()
            }
        }
    }

    private class CoordinatedHandler<T : ProductPermission>(
        private val coordinator: PromptCoordinator,
    ) : ProductPermissionHandler<T> {
        override suspend fun isGranted(productId: ProductId, permission: T) = false
        override suspend fun request(productId: ProductId, permission: T) = coordinator.park() != PermissionDecision.Deny
        override suspend fun revoke(productId: ProductId, permission: T) = Unit
    }

    private class CoordinatedRequester(
        private val coordinator: PromptCoordinator,
    ) : ProductPermissionRequester {
        override suspend fun prompt(productId: ProductId, permission: ProductPermission) = coordinator.park()
        override suspend fun promptBatched(productId: ProductId, permissions: List<RemotePermission>) = coordinator.park()
    }
}
