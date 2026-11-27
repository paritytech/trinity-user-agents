package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.paritytech.polkadotapp.common.data.telemetry.HostLogSink
import io.sentry.SentryAttribute
import io.sentry.SentryAttributes
import io.sentry.SentryLogLevel
import io.sentry.logger.ILoggerApi
import io.sentry.logger.SentryLogParameters
import javax.inject.Inject

class SentryHostLogSink @Inject constructor(
    private val logger: ILoggerApi,
) : HostLogSink {
    override fun log(priority: Int, tag: String, message: String, attributes: Map<String, Any>) {
        val fields = attributes.map { (key, value) -> SentryAttribute.named(key, value) } +
            SentryAttribute.stringAttribute(TAG_ATTRIBUTE, tag)
        logger.log(
            priority.toSentryLogLevel(),
            SentryLogParameters.create(SentryAttributes.of(*fields.toTypedArray())),
            message,
        )
    }

    internal companion object {
        const val TAG_ATTRIBUTE = "tag"
    }
}

// The iOS host has no fatal level, so an assert lands at error on both hosts.
internal fun Int.toSentryLogLevel(): SentryLogLevel = when (this) {
    Log.ERROR, Log.ASSERT -> SentryLogLevel.ERROR
    Log.WARN -> SentryLogLevel.WARN
    else -> SentryLogLevel.INFO
}
