package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import uniffi.truapi.ProductId
import java.util.Objects
import java.util.concurrent.atomic.AtomicInteger

/**
 * Per-product opaque identifier for a push notification.
 *
 * Uniqueness is scoped to a single product: two products may both observe the same numeric value
 * referring to unrelated notifications. Host-wide uniqueness is enforced at the DAO row level via
 * the composite primary key (productId, notificationId).
 */
// TODO not inline because of mockito matchers in RealProductNotificationSchedulerTest
// Waiting for migration to mockk
class NotificationId(val value: Int) {
    internal companion object {
        /** Keeps two notifications a product asks for in the same millisecond apart. */
        private val sequence = AtomicInteger()

        fun generate(productId: ProductId): NotificationId {
            return NotificationId(
                Objects.hash(productId.value, System.currentTimeMillis(), sequence.incrementAndGet())
            )
        }
    }
}
