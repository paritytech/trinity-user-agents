package io.paritytech.polkadotapp.feature_coinage_impl.data.storage

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class RecyclingStrategyStorageTest {
    private val preferences: Preferences = mockk()
    private val storage = SingleValueStorageFactory(preferences).createRecyclingStrategyStorage()

    @Test
    fun `a stored choice survives the new default`() = runBlocking<Unit> {
        withStored("MIN_PRIVACY")

        assertEquals(RecyclingStrategyType.MIN_PRIVACY, storage.requireValue())
        assertEquals(RecyclingStrategyType.MIN_PRIVACY, storage.valueFlow().first())
    }

    private fun withStored(value: String) {
        every { preferences.getString(KEY) } returns value
        every { preferences.keyFlow(KEY) } returns flowOf(KEY)
    }

    private companion object {
        const val KEY = "coinage_recycling_strategy"
    }
}
