package io.paritytech.polkadotapp.app.logging

import io.paritytech.polkadotapp.common.data.telemetry.CoreEventSink
import io.sentry.SentryAttribute
import io.sentry.SentryAttributes
import io.sentry.logger.ILoggerApi
import io.sentry.logger.SentryLogParameters
import javax.inject.Inject

class SentryCoreEventSink @Inject constructor(
    private val logger: ILoggerApi,
) : CoreEventSink {
    override fun log(priority: Int, marker: String, attributes: Map<String, Any>) {
        val fields = attributes.map { (key, value) -> SentryAttribute.named(key, value) } +
            SentryAttribute.stringAttribute(TAG_ATTRIBUTE, CORE_TAG)
        logger.log(
            priority.toSentryLogLevel(),
            SentryLogParameters.create(SentryAttributes.of(*fields.toTypedArray())),
            marker,
        )
    }

    private companion object {
        const val TAG_ATTRIBUTE = "tag"
        const val CORE_TAG = "truapi.core"
    }
}
