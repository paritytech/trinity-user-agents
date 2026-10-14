package io.paritytech.polkadotapp.feature_wallet_impl.data.config

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val APP_SHARING_URL_KEY = "app_sharing_url"
private const val APP_SHARING_URL = "https://example.com/app"

class RealAppSharingConfigRepositoryTest {
    private val remoteConfigService: RemoteConfigService = mockk()

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
        coEvery { remoteConfigService.getSyncedString(APP_SHARING_URL_KEY) } returns Result.success(url)
    }

    private suspend fun withSyncedReadFailing() {
        coEvery { remoteConfigService.getSyncedString(APP_SHARING_URL_KEY) } returns Result.failure(IllegalStateException("not synced"))
    }

    private suspend fun withSyncNeverCompleting() {
        coEvery { remoteConfigService.getSyncedString(APP_SHARING_URL_KEY) } coAnswers { awaitCancellation() }
    }

    private fun assertFailure(result: Result<String>) {
        assertTrue("expected a failure but was $result", result.isFailure)
    }
}
