package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.sentry.SentryAttribute
import io.sentry.SentryAttributes
import io.sentry.SentryLogLevel
import io.sentry.logger.ILoggerApi
import io.sentry.logger.SentryLogParameters
import timber.log.Timber
import javax.inject.Inject

class SentryLogTree @Inject constructor(
    private val logger: ILoggerApi,
) : Timber.Tree() {
    override fun isLoggable(tag: String?, priority: Int): Boolean =
        priority >= Log.INFO && tag?.startsWith(TAG_PREFIX) == true

    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
        val attributes = SentryAttributes.of(SentryAttribute.stringAttribute(TAG_ATTRIBUTE, tag))
        logger.log(priority.toSentryLogLevel(), SentryLogParameters.create(attributes), message)
    }

    private companion object {
        const val TAG_PREFIX = "truapi."
        const val TAG_ATTRIBUTE = "tag"
    }
}

// The iOS host has no fatal level, so an assert lands at error on both hosts.
internal fun Int.toSentryLogLevel(): SentryLogLevel = when (this) {
    Log.ERROR, Log.ASSERT -> SentryLogLevel.ERROR
    Log.WARN -> SentryLogLevel.WARN
    else -> SentryLogLevel.INFO
}
