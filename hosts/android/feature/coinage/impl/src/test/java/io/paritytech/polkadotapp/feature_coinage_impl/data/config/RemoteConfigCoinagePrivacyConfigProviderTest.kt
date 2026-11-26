package io.paritytech.polkadotapp.feature_coinage_impl.data.config

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.AnonymityRequirements
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinagePrivacyConfig
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherReadiness
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getJsonObject
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.minutes

class RemoteConfigCoinagePrivacyConfigProviderTest {
    private val remoteConfigService: RemoteConfigService = mockk()
    private val provider = RemoteConfigCoinagePrivacyConfigProvider(
        remoteConfigService,
        TestCoroutineDispatchers(Dispatchers.Unconfined),
    )

    @Test
    fun `a remote read failure falls back to the bundled defaults`() = runBlocking<Unit> {
        withRemote(Result.failure(IllegalStateException("not activated")))

        assertEquals(BUNDLED, provider.privacyConfigOrDefault())
    }

    @Test
    fun `the first update is the activated value, without waiting for a sync`() = runBlocking<Unit> {
        withRemote(Result.success(null))
        coEvery { remoteConfigService.getSyncedJsonObject<CoinagePrivacyConfigRemote?>(CONFIG_KEY) } coAnswers {
            CompletableDeferred<Nothing>().await()
        }

        assertEquals(BUNDLED, provider.privacyConfigUpdates().first())
    }

    @Test
    fun `a value the first sync activates follows the stale one`() = runBlocking<Unit> {
        withRemote(Result.success(null))
        withSynced(Result.success(PUBLISHED))

        val updates = provider.privacyConfigUpdates().toList()

        assertEquals(2, updates.size)
        assertEquals(BUNDLED, updates.first())
        assertEquals(AnonymityRequirements(5.minutes, 4), updates.last().balanced.anonymityRequirements)
    }

    @Test
    fun `a sync that changes nothing is not re-emitted`() = runBlocking<Unit> {
        withRemote(Result.success(PUBLISHED))
        withSynced(Result.success(PUBLISHED))

        val updates = provider.privacyConfigUpdates().toList()

        assertEquals(1, updates.size)
        assertEquals(AnonymityRequirements(5.minutes, 4), updates.single().balanced.anonymityRequirements)
    }

    private fun withSynced(result: Result<CoinagePrivacyConfigRemote?>) {
        coEvery { remoteConfigService.getSyncedJsonObject<CoinagePrivacyConfigRemote?>(CONFIG_KEY) } returns result
    }

    private fun withRemote(result: Result<CoinagePrivacyConfigRemote?>) {
        coEvery { remoteConfigService.getJsonObject<CoinagePrivacyConfigRemote?>(CONFIG_KEY) } returns result
    }

    private companion object {
        const val CONFIG_KEY = "coinage_privacy_config"

        val PUBLISHED = CoinagePrivacyConfigRemote(
            balanced = PrivacyModeRemote(ringFillPercent = 30, anonymityMinutes = 5, anonymityMembers = 4),
            maxPrivacy = PrivacyModeRemote(ringFillPercent = 80, anonymityMinutes = 7, anonymityMembers = 12),
        )

        val BUNDLED = CoinagePrivacyConfig(
            balanced = VoucherReadiness(20.percents, AnonymityRequirements(anonymityTime = 10.minutes, anonymityMembers = 8)),
            maxPrivacy = VoucherReadiness(90.percents, AnonymityRequirements(anonymityTime = 10.minutes, anonymityMembers = 16)),
        )
    }
}
