package io.paritytech.polkadotapp.common.utils

import org.junit.Assert.assertFalse
import org.junit.Test

class FeatureFlagsTest {
    @Test
    fun `Browse and the connectivity indicator never share the docklet`() {
        assertFalse(FeatureOption.BROWSE_TAB.isEnabled && FeatureOption.TAB_BAR_CONNECTIVITY_INDICATOR.isEnabled)
    }
}
