package io.paritytech.polkadotapp.feature_settings_impl.data.storage

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class RealDockletAutoHideStorageTest {
    private val stored = mutableMapOf<String, Boolean>()
    private val preferences = mockk<Preferences> {
        every { getBoolean(any(), any()) } answers { stored[firstArg()] ?: secondArg() }
        every { booleanFlow(any(), any()) } answers { MutableStateFlow(stored[firstArg()] ?: secondArg()) }
        every { putBoolean(any(), any()) } answers { stored[firstArg()] = secondArg() }
    }
    private val storage = RealDockletAutoHideStorage(preferences)

    @Test
    fun `auto-hide is enabled when nothing was stored`() = runBlocking<Unit> {
        assertEquals(true to true, storage.isEnabled() to storage.observeEnabled().first())
    }

    @Test
    fun `turning auto-hide off is stored and read back`() = runBlocking<Unit> {
        storage.setEnabled(false)

        verify(exactly = 1) { preferences.putBoolean("docklet_auto_hide_enabled", false) }
        assertEquals(false to false, storage.isEnabled() to storage.observeEnabled().first())
    }
}
