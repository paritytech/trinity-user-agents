package io.paritytech.polkadotapp.feature_wallet_impl.data.config

import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.invocation.Invocation
import kotlin.coroutines.Continuation
import kotlin.coroutines.intrinsics.COROUTINE_SUSPENDED
import kotlin.coroutines.startCoroutine

private const val APP_SHARING_URL_KEY = "app_sharing_url"
private const val APP_SHARING_URL = "https://example.com/app"

class RealAppSharingConfigRepositoryTest {
    private val remoteConfigService: RemoteConfigService = mock(RemoteConfigService::class.java)

    private val repository = RealAppSharingConfigRepository(remoteConfigService)

    @Test
    fun `returns the synced url when it is set`() = runBlocking<Unit> {
        withSyncedUrl(APP_SHARING_URL)

        assertEquals(Result.success(APP_SHARING_URL), repository.getAppSharingUrl())
    }

    @Test
    fun `fails when the key is unset`() = runBlocking<Unit> {
        withSyncedUrl("")

        assertFailure(repository.getAppSharingUrl())
    }

    @Test
    fun `fails when the session never syncs`() = runBlocking<Unit> {
        withSyncNeverCompleting()

        assertFailure(repository.getAppSharingUrl())
    }

    @Test
    fun `fails when the synced read fails`() = runBlocking<Unit> {
        withSyncedReadFailing()

        assertFailure(repository.getAppSharingUrl())
    }

    private suspend fun withSyncedUrl(url: String) {
        whenever(remoteConfigService.getSyncedString(APP_SHARING_URL_KEY)).thenReturn(Result.success(url))
    }

    private suspend fun withSyncedReadFailing() {
        whenever(remoteConfigService.getSyncedString(APP_SHARING_URL_KEY))
            .thenReturn(Result.failure(IllegalStateException("not synced")))
    }

    private suspend fun withSyncNeverCompleting() {
        whenever(remoteConfigService.getSyncedString(APP_SHARING_URL_KEY)).thenAnswer { invocation ->
            @Suppress("UNCHECKED_CAST")
            val continuation = (invocation as Invocation).rawArguments.last() as Continuation<Result<String>>
            val waitForever: suspend () -> Result<String> = { awaitCancellation() }
            waitForever.startCoroutine(continuation)
            COROUTINE_SUSPENDED
        }
    }

    private fun assertFailure(result: Result<String>) {
        assertTrue("expected a failure but was $result", result.isFailure)
    }
}
