package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.net.Network
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.WebSocketChainProvider
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Test

class ChainSocketsNetworkSwitchTest {
    private val wifi = mockk<Network>()
    private val cellular = mockk<Network>()
    private val currentDefault = MutableStateFlow<Network?>(wifi)
    private val networkStateService = mockk<NetworkStateService> { every { defaultNetwork } returns currentDefault }
    private val chainProvider = mockk<WebSocketChainProvider>(relaxUnitFun = true)

    @Test
    fun `the sockets stay open on the network they were opened on`() = runTest {
        backgroundScope.closeChainSocketsOnNetworkSwitch(networkStateService, chainProvider)
        runCurrent()

        verify(exactly = 0) { chainProvider.closeAll() }
    }

    @Test
    fun `a switch to another network closes the sockets`() = runTest {
        backgroundScope.closeChainSocketsOnNetworkSwitch(networkStateService, chainProvider)
        runCurrent()

        currentDefault.value = cellular
        runCurrent()

        verify(exactly = 1) { chainProvider.closeAll() }
    }
}
