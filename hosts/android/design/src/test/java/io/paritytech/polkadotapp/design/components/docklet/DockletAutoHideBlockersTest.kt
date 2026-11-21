package io.paritytech.polkadotapp.design.components.docklet

import org.junit.Assert.assertTrue
import org.junit.Test

class DockletAutoHideBlockersTest {
    @Test
    fun `auto-hide turned off in settings blocks it`() {
        assertTrue(blockers(enabled = false).blocked)
    }

    private fun blockers(enabled: Boolean) = DockletAutoHideBlockers(
        enabled = enabled,
        shownAtRest = true,
        searchOpen = false,
        keyboardUp = false,
        tipShown = false,
        panelExpanded = false,
        touchExploration = false,
        resumed = true,
    )
}
