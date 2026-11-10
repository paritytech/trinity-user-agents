package io.paritytech.polkadotapp.common.presentation.tabbar

import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import org.junit.Assert.assertEquals
import org.junit.Test

class DockletSlotTapTest {
    @Test
    fun `toggles the apps grid when Browse is tapped on Browse's main screen`() {
        assertEquals(DockletSlotAction.ToggleApps, tap(Browse, current = BottomTab.EXPLORE, onMain = true))
    }

    @Test
    fun `returns to Browse when Browse is tapped with a screen pushed over it`() {
        assertEquals(SelectBrowse, tap(Browse, current = BottomTab.EXPLORE, onMain = false))
    }

    @Test
    fun `switches to Browse without the grid when Browse is tapped while Chats is current`() {
        assertEquals(SelectBrowse, tap(Browse, current = BottomTab.CHATS, onMain = true))
    }

    @Test
    fun `switches to Browse when Browse is tapped from a screen pushed over Chats`() {
        assertEquals(SelectBrowse, tap(Browse, current = BottomTab.CHATS, onMain = false))
    }

    @Test
    fun `selects Chats when Chats is tapped while Chats is current`() {
        assertEquals(SelectChats, tap(Chats, current = BottomTab.CHATS, onMain = true))
    }

    @Test
    fun `selects Chats when Chats is tapped while Browse is current`() {
        assertEquals(SelectChats, tap(Chats, current = BottomTab.EXPLORE, onMain = true))
    }

    @Test
    fun `toggles the chain-health panel when Network Status is tapped`() {
        assertEquals(
            DockletSlotAction.ToggleNetworkStatus,
            tap(DockletSlotTarget.NetworkStatus, current = BottomTab.CHATS, onMain = true),
        )
    }

    @Test
    fun `only closes the card when Browse is tapped on Browse's main screen while the card is open`() {
        assertEquals(DockletSlotAction.CloseCard, tap(Browse, current = BottomTab.EXPLORE, onMain = true, cardOpen = true))
    }

    @Test
    fun `only closes the card when Network Status is tapped while the card is open`() {
        assertEquals(
            DockletSlotAction.CloseCard,
            tap(DockletSlotTarget.NetworkStatus, current = BottomTab.CHATS, onMain = true, cardOpen = true),
        )
    }

    @Test
    fun `only closes the card when Settings is tapped while the card is open`() {
        assertEquals(
            DockletSlotAction.CloseCard,
            tap(DockletSlotTarget.Tab(BottomTab.SETTINGS), current = BottomTab.CHATS, onMain = true, cardOpen = true),
        )
    }

    @Test
    fun `highlights Network Status when the chain-health panel is open`() {
        assertEquals(
            DockletSlotTarget.NetworkStatus,
            dockletHighlightedSlot(currentTab = BottomTab.CHATS, networkStatusExpanded = true),
        )
    }

    @Test
    fun `highlights the current tab when the chain-health panel is closed`() {
        assertEquals(Chats, dockletHighlightedSlot(currentTab = BottomTab.CHATS, networkStatusExpanded = false))
    }

    private fun tap(target: DockletSlotTarget, current: BottomTab, onMain: Boolean, cardOpen: Boolean = false) =
        dockletSlotTap(target, current, onMain, cardOpen)

    private companion object {
        val Browse = DockletSlotTarget.Tab(BottomTab.EXPLORE)
        val Chats = DockletSlotTarget.Tab(BottomTab.CHATS)
        val SelectBrowse = DockletSlotAction.SelectTab(BottomTab.EXPLORE)
        val SelectChats = DockletSlotAction.SelectTab(BottomTab.CHATS)
    }
}
