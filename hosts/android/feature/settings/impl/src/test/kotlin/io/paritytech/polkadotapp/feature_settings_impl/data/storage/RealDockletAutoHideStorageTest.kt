package io.paritytech.polkadotapp.feature_settings_impl.data.storage

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class RealDockletAutoHideStorageTest {
    private val preferences = mockk<Preferences> {
        every { getBoolean(any(), any()) } answers { secondArg() }
        every { booleanFlow(any(), any()) } answers { MutableStateFlow(secondArg()) }
    }
    private val storage = RealDockletAutoHideStorage(preferences)

    @Test
    fun `auto-hide is enabled when nothing was stored`() = runBlocking<Unit> {
        assertEquals(true to true, storage.isEnabled() to storage.observeEnabled().first())
    }
}
