package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.paritytech.polkadotapp.common.data.telemetry.HostLogSink
import timber.log.Timber
import javax.inject.Inject

/**
 * Forwards `truapi.*` lines at info and above to the [HostLogSink]. Every other line stays on the
 * device.
 *
 * How many of the forwarded lines reach Sentry is decided at the SDK's own gate, in [HostLogFilter].
 */
class HostLogSinkTree @Inject constructor(
    private val sink: HostLogSink,
) : Timber.Tree() {
    override fun isLoggable(tag: String?, priority: Int): Boolean =
        priority >= Log.INFO && tag?.startsWith(HostLogFilter.TAG_PREFIX) == true

    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
        sink.log(priority, tag.orEmpty(), message, emptyMap())
    }
}
