package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.feature_scan_api.domain.PostParseAction
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PocketScanContentParserTest {
    /**
     * The three refusals the host can give a Pocket link — no Pocket, unknown card, malformed — reach
     * the scanner as a message it has no way to show. Reported as a failure they at least raise the
     * scanner's own invalid-code dialog, which is also the only thing that lets the user scan again;
     * carried as "do nothing" they leave a scanner that took the code and then sat there.
     */
    @Test
    fun `a refusal the scanner cannot voice is reported rather than accepted silently`() {
        val outcome = DeeplinkProcessingOutcome.ShowMessage("This product has no Pocket")

        val action = outcome.asScanAction()

        assertTrue(action.isFailure)
        assertEquals("This product has no Pocket", action.exceptionOrNull()?.message)
    }

    @Test
    fun `a link the host accepts leaves the scanner for wherever it points`() {
        var opened = false
        val outcome = DeeplinkProcessingOutcome.Navigate { opened = true }

        val action = outcome.asScanAction().getOrThrow()

        (action as PostParseAction.BackAndThen).postBackNavigation()
        assertTrue(opened)
    }

    @Test
    fun `a link that asks for nothing asks the scanner for nothing`() {
        assertEquals(PostParseAction.Nothing, DeeplinkProcessingOutcome.NoOp.asScanAction().getOrThrow())
    }
}
