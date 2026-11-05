package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.sentry.Sentry
import io.sentry.SentryAttribute
import io.sentry.SentryAttributes
import io.sentry.SentryLogLevel
import io.sentry.logger.ILoggerApi
import io.sentry.logger.SentryLogParameters
import timber.log.Timber

class SentryLogTree(
    private val logger: ILoggerApi = Sentry.logger(),
) : Timber.Tree() {
    override fun isLoggable(tag: String?, priority: Int): Boolean =
        priority >= Log.INFO && tag?.startsWith(TAG_PREFIX) == true

    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
        val attributes = SentryAttributes.of(SentryAttribute.stringAttribute(TAG_ATTRIBUTE, tag))
        logger.log(priority.toSentryLogLevel(), SentryLogParameters.create(attributes), message)
    }

    private fun Int.toSentryLogLevel(): SentryLogLevel = when (this) {
        Log.ASSERT -> SentryLogLevel.FATAL
        Log.ERROR -> SentryLogLevel.ERROR
        Log.WARN -> SentryLogLevel.WARN
        else -> SentryLogLevel.INFO
    }

    private companion object {
        const val TAG_PREFIX = "truapi."
        const val TAG_ATTRIBUTE = "tag"
    }
}
