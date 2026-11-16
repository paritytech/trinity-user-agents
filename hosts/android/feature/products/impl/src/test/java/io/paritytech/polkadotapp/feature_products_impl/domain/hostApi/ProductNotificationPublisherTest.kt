package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationReason
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductNotificationPublisherTest {
    @Test
    fun `notifications off for the app is notifications_disabled`() {
        assertEquals(ProductNotificationReason.NOTIFICATIONS_DISABLED, suppressedReason(notificationsEnabled = false))
    }

    @Test
    fun `a blocked channel is channel_blocked`() {
        assertEquals(ProductNotificationReason.CHANNEL_BLOCKED, suppressedReason(notificationsEnabled = true))
    }
}
