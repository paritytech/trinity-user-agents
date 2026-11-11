@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkConstructor
import io.mockk.mockkStatic
import io.mockk.unmockkAll
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationStep
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotification
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotificationRepository
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.ProductId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealProductNotificationSchedulerTelemetryTest {
    private val alarmManager = mockk<AlarmManager>(relaxed = true)
    private val context = mockk<Context> {
        every { getSystemService(AlarmManager::class.java) } returns alarmManager
    }
    private val repository = mockk<ScheduledProductNotificationRepository>(relaxUnitFun = true)
    private val timeProvider = mockk<TimeProvider>()
    private val originProvider = HostEventOriginProvider {
        HostEventOrigin(
            appVariant = "nightly",
            appFlavor = "gp",
            nativeAppVersion = "1.2.3",
            platform = "android",
            osVersion = "35",
            appBuild = 42,
        )
    }
    private val captured = mutableListOf<HostEvent>()
    private val scheduler = RealProductNotificationScheduler(
        context,
        repository,
        ProductNotificationTelemetry(timeProvider, originProvider, HostEventCapture { captured += it() }),
    )

    private val productId = ProductId.parse("acme.dot").getOrThrow()
    private val otherProductId = ProductId.parse("other.dot").getOrThrow()
    private val scheduledAt = Instant.fromEpochMilliseconds(DUE)

    @Before
    fun setUp() {
        mockkConstructor(Intent::class)
        every { anyConstructed<Intent>().setAction(any()) } returns mockk()
        every { anyConstructed<Intent>().putExtra(any<String>(), any<String>()) } returns mockk()
        every { anyConstructed<Intent>().putExtra(any<String>(), any<Int>()) } returns mockk()
        mockkStatic(PendingIntent::class)
        every { PendingIntent.getBroadcast(any(), any(), any(), any()) } returns mockk()
        coEvery { repository.countAll() } returns 0
        coEvery { repository.exists(any(), any()) } returns false
    }

    @After
    fun tearDown() {
        unmockkAll()
    }

    @Test
    fun `an accepted booking records scheduled with its id and due time`() = runBlocking {
        // When
        val id = scheduler.schedule(productId, "hello", deeplink = "polkadot://secret", scheduledAt).getOrThrow()

        // Then
        assertEquals(
            listOf(
                event(
                    ProductNotificationStep.SCHEDULED,
                    tags = productTags + ("product_notification.id" to id.value.toUInt().toString()),
                    extras = baseExtras + ("product_notification.scheduled_at_unix_ms" to DUE),
                ),
            ),
            captured,
        )
    }

    @Test
    fun `a full table records not_scheduled limit_reached without an id`() = runBlocking {
        // Given
        coEvery { repository.countAll() } returns 64

        // When
        val result = scheduler.schedule(productId, "hello", deeplink = null, scheduledAt)

        // Then
        assertSame(ProductPushNotificationException.ScheduleLimitReached, result.exceptionOrNull())
        assertEquals(listOf(notScheduled(id = null, reason = "limit_reached")), captured)
    }

    @Test
    fun `an id collision records not_scheduled id_collision with the id`() = runBlocking {
        // Given
        val ids = mutableListOf<NotificationId>()
        coEvery { repository.exists(productId, capture(ids)) } returns true

        // When
        val result = scheduler.schedule(productId, "hello", deeplink = null, scheduledAt)

        // Then
        assertTrue(result.isFailure)
        assertEquals(listOf(notScheduled(ids.single().value, "id_collision")), captured)
    }

    @Test
    fun `a refused alarm records add_failed and returns the same failure`() = runBlocking {
        // Given
        val refusal = SecurityException("exact alarms are off")
        every { alarmManager.setAndAllowWhileIdle(any(), any(), any()) } throws refusal
        val ids = mutableListOf<NotificationId>()
        coEvery { repository.exists(productId, capture(ids)) } returns false

        // When
        val result = scheduler.schedule(productId, "hello", deeplink = null, scheduledAt)

        // Then
        assertSame(refusal, result.exceptionOrNull())
        assertEquals(listOf(notScheduled(ids.single().value, "add_failed")), captured)
    }

    @Test
    fun `a failed insert records store_failed and returns the same failure`() = runBlocking {
        // Given
        val storeError = IllegalStateException("disk full")
        coEvery { repository.insert(any()) } throws storeError
        val ids = mutableListOf<NotificationId>()
        coEvery { repository.exists(productId, capture(ids)) } returns false

        // When
        val result = scheduler.schedule(productId, "hello", deeplink = null, scheduledAt)

        // Then
        assertSame(storeError, result.exceptionOrNull())
        assertEquals(listOf(notScheduled(ids.single().value, "store_failed")), captured)
    }

    @Test
    fun `a cancelled insert records nothing`() = runBlocking {
        // Given
        val cancellation = CancellationException("closed")
        coEvery { repository.insert(any()) } throws cancellation

        // When
        val result = scheduler.schedule(productId, "hello", deeplink = null, scheduledAt)

        // Then
        assertSame(cancellation, result.exceptionOrNull())
        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `cancel of a stored booking records cancelled`() = runBlocking {
        // Given
        coEvery { repository.exists(productId, any()) } returns true

        // When
        scheduler.cancel(productId, NotificationId(7))

        // Then
        assertEquals(listOf(cancelled(productTags, id = 7)), captured)
    }

    @Test
    fun `cancel of an unknown id records nothing`() = runBlocking {
        // When
        scheduler.cancel(productId, NotificationId(7))

        // Then
        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `a cancel whose delete fails records nothing`() = runBlocking {
        // Given
        coEvery { repository.exists(productId, any()) } returns true
        coEvery { repository.delete(any(), any()) } throws IllegalStateException("disk full")

        // When
        val result = scheduler.cancel(productId, NotificationId(7))

        // Then
        assertTrue(result.isFailure)
        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `uninstall records one cancelled per stored row of that product`() = runBlocking {
        // Given
        coEvery { repository.getAll() } returns listOf(
            stored(productId, 7),
            stored(otherProductId, 8),
            stored(productId, 9),
        )

        // When
        scheduler.cancelAllForProduct(productId)

        // Then
        assertEquals(listOf(cancelled(productTags, id = 7), cancelled(productTags, id = 9)), captured)
    }

    private fun stored(product: ProductId, id: Int) = ScheduledProductNotification(
        productId = product,
        notificationId = NotificationId(id),
        text = "hello",
        deeplink = null,
        scheduledAt = scheduledAt,
    )

    private fun notScheduled(id: Int?, reason: String) = event(
        ProductNotificationStep.NOT_SCHEDULED,
        tags = productTags +
            listOfNotNull(id?.let { "product_notification.id" to it.toUInt().toString() }) +
            ("product_notification.reason" to reason),
        extras = baseExtras,
    )

    private fun cancelled(tags: Map<String, String>, id: Int) = event(
        ProductNotificationStep.CANCELLED,
        tags + ("product_notification.id" to id.toUInt().toString()),
        baseExtras,
    )

    private fun event(step: ProductNotificationStep, tags: Map<String, String>, extras: Map<String, Any>) =
        HostEvent(name = step.eventName, tags = tags, extras = extras)

    private companion object {
        const val DUE = 2_000_000_000_000L

        val productTags = mapOf(
            "host.platform" to "android",
            "host.app_variant" to "nightly",
            "host.app_flavor" to "gp",
            "host.native_app_version" to "1.2.3",
            "product_notification.product_id" to "acme.dot",
        )
        val baseExtras = mapOf<String, Any>("host.app_build" to 42L)
    }
}
