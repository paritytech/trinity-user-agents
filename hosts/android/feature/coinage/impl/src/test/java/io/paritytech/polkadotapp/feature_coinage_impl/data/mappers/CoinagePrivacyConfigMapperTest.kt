package io.paritytech.polkadotapp.feature_coinage_impl.data.mappers

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.AnonymityRequirements
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinagePrivacyConfig
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherReadiness
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinagePrivacyConfigRemote
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.PrivacyModeRemote
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.minutes

class CoinagePrivacyConfigMapperTest {
    @Test
    fun `an unset key maps to the bundled defaults`() {
        val unset: CoinagePrivacyConfigRemote? = null

        assertEquals(BUNDLED, unset.toDomain())
    }

    @Test
    fun `an empty object maps to the bundled defaults`() {
        assertEquals(BUNDLED, CoinagePrivacyConfigRemote(balanced = null, maxPrivacy = null).toDomain())
    }

    @Test
    fun `a missing mode takes its own defaults and keeps the other`() {
        val remote = CoinagePrivacyConfigRemote(
            balanced = PrivacyModeRemote(ringFillPercent = 30, anonymityMinutes = 5, anonymityMembers = 4),
            maxPrivacy = null,
        )

        assertEquals(CoinagePrivacyConfig(readiness(30, 5, 4), BUNDLED.maxPrivacy), remote.toDomain())
    }

    @Test
    fun `a missing field takes its own default`() {
        val remote = CoinagePrivacyConfigRemote(
            balanced = PrivacyModeRemote(ringFillPercent = 30, anonymityMinutes = null, anonymityMembers = null),
            maxPrivacy = PrivacyModeRemote(ringFillPercent = null, anonymityMinutes = 7, anonymityMembers = 12),
        )

        assertEquals(CoinagePrivacyConfig(readiness(30, 10, 8), readiness(90, 7, 12)), remote.toDomain())
    }

    @Test
    fun `an out of range field takes its own default`() {
        val cases = listOf(
            PrivacyModeRemote(ringFillPercent = 30, anonymityMinutes = 5, anonymityMembers = 0) to readiness(30, 5, 8),
            PrivacyModeRemote(ringFillPercent = 30, anonymityMinutes = 5, anonymityMembers = -1) to readiness(30, 5, 8),
            PrivacyModeRemote(ringFillPercent = 30, anonymityMinutes = 0, anonymityMembers = 4) to readiness(30, 10, 4),
            PrivacyModeRemote(ringFillPercent = 30, anonymityMinutes = -5, anonymityMembers = 4) to readiness(30, 10, 4),
            PrivacyModeRemote(ringFillPercent = -1, anonymityMinutes = 5, anonymityMembers = 4) to readiness(20, 5, 4),
            PrivacyModeRemote(ringFillPercent = 101, anonymityMinutes = 5, anonymityMembers = 4) to readiness(20, 5, 4),
        )

        cases.forEach { (balanced, expected) ->
            val mapped = CoinagePrivacyConfigRemote(balanced = balanced, maxPrivacy = null).toDomain()

            assertEquals(expected, mapped.balanced)
        }
    }

    @Test
    fun `a zero ring fill is kept`() {
        val remote = CoinagePrivacyConfigRemote(
            balanced = PrivacyModeRemote(ringFillPercent = 0, anonymityMinutes = 5, anonymityMembers = 4),
            maxPrivacy = null,
        )

        assertEquals(readiness(0, 5, 4), remote.toDomain().balanced)
    }

    @Test
    fun `the published json field names map onto the config`() {
        val custom = """{"balanced":{"ringFillPercent":30,"anonymityMinutes":5,"anonymityMembers":4},""" +
            """"maxPrivacy":{"ringFillPercent":80,"anonymityMinutes":7,"anonymityMembers":12}}"""

        assertEquals(CoinagePrivacyConfig(readiness(30, 5, 4), readiness(80, 7, 12)), custom.parsedConfig())
    }

    private fun String.parsedConfig() = Gson().fromJson(this, CoinagePrivacyConfigRemote::class.java).toDomain()

    private fun readiness(ringFillPercent: Int, anonymityMinutes: Int, anonymityMembers: Int) = VoucherReadiness(
        requiredRingFill = ringFillPercent.percents,
        anonymityRequirements = AnonymityRequirements(anonymityMinutes.minutes, anonymityMembers),
    )

    private companion object {
        val BUNDLED = CoinagePrivacyConfig(
            balanced = VoucherReadiness(20.percents, AnonymityRequirements(anonymityTime = 10.minutes, anonymityMembers = 8)),
            maxPrivacy = VoucherReadiness(90.percents, AnonymityRequirements(anonymityTime = 10.minutes, anonymityMembers = 16)),
        )
    }
}
