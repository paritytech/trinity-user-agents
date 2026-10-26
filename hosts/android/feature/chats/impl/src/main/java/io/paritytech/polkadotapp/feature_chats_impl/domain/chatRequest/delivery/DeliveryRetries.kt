package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.common.utils.RetryDelayStrategy
import io.paritytech.polkadotapp.common.utils.exponentialRetryDelay
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningDeclined
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

internal val DELIVERY_RETRY_DELAY: RetryDelayStrategy = exponentialRetryDelay(initialDelay = 2.seconds, maxDelay = 5.minutes)

// Only the user's decline ends a delivery, as on iOS; every other failure, an underivable request id included, is retried.
internal fun Throwable.endsDelivery(): Boolean {
    return this is IdentitySigningDeclined
}
