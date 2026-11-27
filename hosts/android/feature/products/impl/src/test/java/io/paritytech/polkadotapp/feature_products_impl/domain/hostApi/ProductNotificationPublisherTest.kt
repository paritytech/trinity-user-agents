package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import io.paritytech.polkadotapp.common.presentation.notifications.PublishOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationReason
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductNotificationPublisherTest {
    @Test
    fun `each publish outcome is shown or suppressed for its own reason`() {
        val reasons = PublishOutcome.entries.associateWith { it.suppressedReason }

        assertEquals(
            mapOf(
                PublishOutcome.POSTED to null,
                PublishOutcome.NOTIFICATIONS_DISABLED to ProductNotificationReason.NOTIFICATIONS_DISABLED,
                PublishOutcome.CHANNEL_BLOCKED to ProductNotificationReason.CHANNEL_BLOCKED,
            ),
            reasons,
        )
    }
}
