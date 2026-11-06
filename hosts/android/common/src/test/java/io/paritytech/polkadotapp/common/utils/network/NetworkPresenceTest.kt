package io.paritytech.polkadotapp.common.utils.network

import android.net.Network
import io.mockk.mockk
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NetworkPresenceTest {
    private val wifi: Network = mockk()
    private val cellular: Network = mockk()

    private val presence = NetworkPresence()

    @Test
    fun `reads unavailable when no network has been reported`() {
        assertFalse(presence.isAvailable.value)
    }

    @Test
    fun `reads available when a network is reported`() {
        presence.add(wifi)

        assertTrue(presence.isAvailable.value)
    }

    @Test
    fun `reads unavailable when the only network is lost`() {
        presence.add(wifi)

        presence.remove(wifi)

        assertFalse(presence.isAvailable.value)
    }

    @Test
    fun `stays available when one of two networks is lost`() {
        presence.add(wifi)
        presence.add(cellular)

        presence.remove(wifi)

        assertTrue(presence.isAvailable.value)
    }

    @Test
    fun `stays available when a network that was never reported is lost`() {
        presence.add(wifi)

        presence.remove(cellular)

        assertTrue(presence.isAvailable.value)
    }

    @Test
    fun `reads unavailable when a network reported twice is lost once`() {
        presence.add(wifi)
        presence.add(wifi)

        presence.remove(wifi)

        assertFalse(presence.isAvailable.value)
    }
}
