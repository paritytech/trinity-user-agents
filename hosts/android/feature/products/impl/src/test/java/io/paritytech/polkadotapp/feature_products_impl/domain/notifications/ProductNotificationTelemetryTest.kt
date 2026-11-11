@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationReason
import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationStep
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotification
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class ProductNotificationTelemetryTest {
    private val product = ProductId.parse("acme.dot").getOrThrow()
    private val booking = NotificationId(7)
    private val timeProvider = mockk<TimeProvider> {
        every { now() } returns Instant.fromEpochMilliseconds(NOW)
    }
    private val originProvider = mockk<HostEventOriginProvider> {
        every { current() } returns HostEventOrigin(
            appVariant = "nightly",
            appFlavor = "gp",
            nativeAppVersion = "1.2.3",
            platform = "android",
            osVersion = "35",
            appBuild = 42,
        )
    }

    @Test
    fun `scheduled carries the booking id and the due time`() {
        // Given
        val notification = ScheduledProductNotification(
            productId = product,
            notificationId = booking,
            text = "hello",
            deeplink = "polkadot://secret",
            scheduledAt = Instant.fromEpochMilliseconds(DUE),
        )

        // When
        val events = captured { scheduled(notification) }

        // Then
        assertEquals(
            listOf(
                event(
                    ProductNotificationStep.SCHEDULED,
                    tags = withBooking,
                    extras = baseExtras + ("product_notification.scheduled_at_unix_ms" to DUE),
                ),
            ),
            events,
        )
    }

    @Test
    fun `not scheduled without an id has no id tag`() {
        // When
        val events = captured { notScheduled(product, notificationId = null, ProductNotificationReason.LIMIT_REACHED) }

        // Then
        assertEquals(
            listOf(
                event(
                    ProductNotificationStep.NOT_SCHEDULED,
                    tags = productTags + ("product_notification.reason" to "limit_reached"),
                    extras = baseExtras,
                ),
            ),
            events,
        )
    }

    @Test
    fun `shown carries the due time and the fire time`() {
        // When
        val events = captured { shown(product, booking, Instant.fromEpochMilliseconds(DUE)) }

        // Then
        assertEquals(
            listOf(
                event(
                    ProductNotificationStep.SHOWN,
                    tags = withBooking,
                    extras = baseExtras + mapOf(
                        "product_notification.scheduled_at_unix_ms" to DUE,
                        "product_notification.delivered_at_unix_ms" to NOW,
                    ),
                ),
            ),
            events,
        )
    }

    @Test
    fun `suppressed carries the reason`() {
        // When
        val events = captured {
            suppressed(product, booking, scheduledAt = null, ProductNotificationReason.NOTIFICATIONS_DISABLED)
        }

        // Then
        assertEquals(
            listOf(
                event(
                    ProductNotificationStep.SUPPRESSED,
                    tags = withBooking + ("product_notification.reason" to "notifications_disabled"),
                    extras = baseExtras + ("product_notification.delivered_at_unix_ms" to NOW),
                ),
            ),
            events,
        )
    }

    @Test
    fun `cancelled carries only the product and the id`() {
        // When
        val events = captured { cancelled(product, booking) }

        // Then
        assertEquals(listOf(event(ProductNotificationStep.CANCELLED, withBooking, baseExtras)), events)
    }

    @Test
    fun `a negative booking id is tagged as the unsigned number the product holds`() {
        // When
        val events = captured { cancelled(product, NotificationId(-7)) }

        // Then
        assertEquals(
            listOf(
                event(
                    ProductNotificationStep.CANCELLED,
                    productTags + ("product_notification.id" to "4294967289"),
                    baseExtras,
                ),
            ),
            events,
        )
    }

    @Test
    fun `the step names are the ones the Sentry query reads`() {
        // Then
        assertEquals(
            listOf(
                "host.product_notification.scheduled",
                "host.product_notification.not_scheduled",
                "host.product_notification.shown",
                "host.product_notification.suppressed",
                "host.product_notification.cancelled",
                "host.product_notification.tapped",
            ),
            ProductNotificationStep.entries.map { it.eventName },
        )
    }

    @Test
    fun `a disabled capture never builds the event`() {
        // Given
        val telemetry = ProductNotificationTelemetry(timeProvider, originProvider, HostEventCapture { })

        // When
        telemetry.shown(product, booking, Instant.fromEpochMilliseconds(DUE))

        // Then
        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 0) { timeProvider.now() }
    }

    private fun captured(report: ProductNotificationTelemetry.() -> Unit): List<HostEvent> {
        val events = mutableListOf<HostEvent>()
        ProductNotificationTelemetry(timeProvider, originProvider, HostEventCapture { events += it() }).report()
        return events
    }

    private fun event(step: ProductNotificationStep, tags: Map<String, String>, extras: Map<String, Any>) =
        HostEvent(name = step.eventName, tags = tags, extras = extras)

    private companion object {
        const val NOW = 1_000_000_000_000L
        const val DUE = 2_000_000_000_000L

        val productTags = mapOf(
            "host.platform" to "android",
            "host.app_variant" to "nightly",
            "host.app_flavor" to "gp",
            "host.native_app_version" to "1.2.3",
            "product_notification.product_id" to "acme.dot",
        )
        val withBooking = productTags + ("product_notification.id" to "7")
        val baseExtras = mapOf<String, Any>("host.app_build" to 42L)
    }
}
