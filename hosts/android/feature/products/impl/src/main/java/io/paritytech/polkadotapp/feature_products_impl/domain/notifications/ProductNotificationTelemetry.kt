@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.telemetry.CriticalIssue
import io.paritytech.polkadotapp.common.data.telemetry.CriticalIssueCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotification
import uniffi.truapi.ProductId
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class ProductNotificationTelemetry @Inject internal constructor(
    private val timeProvider: TimeProvider,
    private val originProvider: HostEventOriginProvider,
    private val capture: HostEventCapture,
    private val issueCapture: CriticalIssueCapture,
) {
    private companion object {
        const val ISSUE_FLOW = "product-notification"
    }

    fun scheduled(notification: ScheduledProductNotification) = report(
        ProductNotificationStep.SCHEDULED,
        notification.productId,
        notification.notificationId,
        scheduledAt = notification.scheduledAt,
    )

    fun notScheduled(
        productId: ProductId,
        notificationId: NotificationId?,
        reason: ProductNotificationReason,
        error: Throwable?,
    ) {
        report(ProductNotificationStep.NOT_SCHEDULED, productId, notificationId, reason)
        reason.issueKind?.let { kind ->
            runCatching { issueCapture.capture(CriticalIssue(flow = ISSUE_FLOW, kind = kind, error = error)) }
                .logFailure("Failed to capture product notification issue")
        }
    }

    fun shown(productId: ProductId, notificationId: NotificationId, scheduledAt: Instant?) =
        report(ProductNotificationStep.SHOWN, productId, notificationId, scheduledAt = scheduledAt, delivered = true)

    fun suppressed(
        productId: ProductId,
        notificationId: NotificationId,
        scheduledAt: Instant?,
        reason: ProductNotificationReason,
    ) = report(ProductNotificationStep.SUPPRESSED, productId, notificationId, reason, scheduledAt, delivered = true)

    fun cancelled(productId: ProductId, notificationId: NotificationId) =
        report(ProductNotificationStep.CANCELLED, productId, notificationId)

    fun tapped(productId: ProductId, notificationId: NotificationId?) =
        report(ProductNotificationStep.TAPPED, productId, notificationId)

    private fun report(
        step: ProductNotificationStep,
        productId: ProductId,
        notificationId: NotificationId?,
        reason: ProductNotificationReason? = null,
        scheduledAt: Instant? = null,
        delivered: Boolean = false,
    ) {
        runCatching {
            capture.capture {
                originProvider.current().productNotification(
                    step = step,
                    productId = productId.value,
                    notificationId = notificationId?.value,
                    reason = reason,
                    scheduledAtMillis = scheduledAt?.toEpochMilliseconds(),
                    deliveredAtMillis = if (delivered) timeProvider.now().toEpochMilliseconds() else null,
                )
            }
        }.logFailure("Failed to capture product notification event")
    }
}
