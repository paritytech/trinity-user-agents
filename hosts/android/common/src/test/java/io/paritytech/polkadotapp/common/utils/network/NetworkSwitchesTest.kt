package io.paritytech.polkadotapp.common.utils.network

import android.net.Network
import io.mockk.mockk
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class NetworkSwitchesTest {
    private val wifi = mockk<Network>()
    private val cellular = mockk<Network>()

    private fun switchesOf(vararg defaults: Network?): List<Network> =
        runBlocking { flowOf(*defaults).networkSwitches().toList() }

    @Test
    fun `the network in use when collection starts is not a switch`() {
        assertEquals(emptyList<Network>(), switchesOf(null, wifi))
    }

    @Test
    fun `each move to another network is a switch`() {
        assertEquals(listOf(cellular, wifi), switchesOf(wifi, cellular, wifi))
    }

    // Sockets on a network that dropped and came back are as good as before it dropped.
    @Test
    fun `getting the same network back after none is not a switch`() {
        assertEquals(emptyList<Network>(), switchesOf(wifi, null, wifi))
    }

    @Test
    fun `coming back on another network after none is a switch`() {
        assertEquals(listOf(cellular), switchesOf(wifi, null, cellular))
    }
}
