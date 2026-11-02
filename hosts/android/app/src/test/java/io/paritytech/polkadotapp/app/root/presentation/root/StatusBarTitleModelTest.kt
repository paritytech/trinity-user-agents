package io.paritytech.polkadotapp.app.root.presentation.root

import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import org.junit.Assert.assertEquals
import org.junit.Test

class StatusBarTitleModelTest {
    @Test
    fun `main shows the selected tab`() {
        BottomTab.entries.forEach { tab ->
            assertEquals(StatusBarTitleModel.Tab(tab), statusBarTitleOf(R.id.mainFragment, tab))
        }
    }

    @Test
    fun `the browser shows the product whatever tab is selected`() {
        BottomTab.entries.forEach { tab ->
            assertEquals(StatusBarTitleModel.Product, statusBarTitleOf(R.id.spaBrowserFragment, tab))
        }
    }

    @Test
    fun `any other screen shows no title`() {
        assertEquals(StatusBarTitleModel.None, statusBarTitleOf(R.id.chatFeedFragment, BottomTab.CHATS))
        assertEquals(StatusBarTitleModel.None, statusBarTitleOf(R.id.ThemeFragment, BottomTab.SETTINGS))
        assertEquals(StatusBarTitleModel.None, statusBarTitleOf(R.id.truapiConfirmationBottomSheet, BottomTab.EXPLORE))
    }

    @Test
    fun `leaving the browser names the tab Main is on again`() {
        val titles = listOf(R.id.mainFragment, R.id.spaBrowserFragment, R.id.mainFragment)
            .map { statusBarTitleOf(it, BottomTab.SETTINGS) }

        assertEquals(
            listOf(StatusBarTitleModel.Tab(BottomTab.SETTINGS), StatusBarTitleModel.Product, StatusBarTitleModel.Tab(BottomTab.SETTINGS)),
            titles,
        )
    }

    @Test
    fun `no title before the first destination is known`() {
        assertEquals(StatusBarTitleModel.None, statusBarTitleOf(null, BottomTab.CHATS))
    }
}
