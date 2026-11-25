package io.paritytech.polkadotapp.app.logging

import android.app.Application
import android.content.pm.PackageManager
import io.sentry.SentryAttribute
import io.sentry.SentryLogEventAttributeValue
import io.sentry.SentryOptions
import io.sentry.android.core.SentryAndroid

/**
 * Starts Sentry with a [HostLogFilter] at its log gate.
 *
 * The SDK calls its log callback on every log whatever route reached the logger, so a log carrying
 * no tag this host writes is turned away there and the rest are limited per tag and per core
 * marker. Every other option still comes from the manifest, which is where the SDK reads them from
 * whether it starts itself or is started here.
 *
 * A build whose manifest carries no DSN starts nothing, which is what keeps a release build dark.
 */
object SentryLogGate {
    /** Start the SDK for this build, with the log gate in place before the first log. */
    fun install(application: Application) {
        if (!startsSentry(application)) return

        val filter = HostLogFilter()
        SentryAndroid.init(application) { options -> options.logs.setBeforeSend(beforeSendLog(filter)) }
    }

    /**
     * The callback the SDK runs on every log. A log carrying no tag this host writes is turned
     * away, and the first log after a minute that turned any away under the same key carries
     * [HostLogFilter.DROPPED_ATTRIBUTE], which counts them.
     */
    internal fun beforeSendLog(filter: HostLogFilter) =
        SentryOptions.Logs.BeforeSendLogCallback { event ->
            val tag = event.attributes?.get(SentryHostLogSink.TAG_ATTRIBUTE)?.value as? String
            when (val verdict = filter.verdict(tag, event.body)) {
                HostLogVerdict.Drop -> null
                is HostLogVerdict.Send -> event.also {
                    if (verdict.dropped > 0) {
                        it.setAttribute(
                            HostLogFilter.DROPPED_ATTRIBUTE,
                            SentryLogEventAttributeValue.fromAttribute(
                                SentryAttribute.integerAttribute(
                                    HostLogFilter.DROPPED_ATTRIBUTE,
                                    verdict.dropped,
                                ),
                            ),
                        )
                    }
                }
            }
        }

    /**
     * Whether this build starts Sentry at all, which is whether its manifest carries a DSN. A
     * package manager that cannot answer leaves the build dark rather than taking it down.
     */
    internal fun startsSentry(application: Application): Boolean =
        !application.sentryDsn().isNullOrBlank()

    /** The DSN this build's manifest carries, or null when it carries none. */
    private fun Application.sentryDsn(): String? =
        runCatching {
            packageManager
                .getApplicationInfo(packageName, PackageManager.GET_META_DATA)
                .metaData
                ?.getString(DSN_META_DATA)
        }.getOrNull()

    private const val DSN_META_DATA = "io.sentry.dsn"
}
