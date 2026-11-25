package io.paritytech.polkadotapp.app.logging

import android.app.Application
import android.content.pm.PackageManager
import io.sentry.SentryAttribute
import io.sentry.SentryLogEventAttributeValue
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
    fun install(application: Application) {
        if (application.sentryDsn().isNullOrBlank()) return

        val filter = HostLogFilter()
        SentryAndroid.init(application) { options ->
            options.logs.setBeforeSend { event ->
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
        }
    }

    private fun Application.sentryDsn(): String? =
        packageManager
            .getApplicationInfo(packageName, PackageManager.GET_META_DATA)
            .metaData
            ?.getString(DSN_META_DATA)

    private const val DSN_META_DATA = "io.sentry.dsn"
}
