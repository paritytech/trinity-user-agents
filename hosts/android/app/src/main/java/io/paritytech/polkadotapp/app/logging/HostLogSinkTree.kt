package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.paritytech.polkadotapp.common.data.telemetry.HostLogSink
import timber.log.Timber
import javax.inject.Inject

class HostLogSinkTree @Inject constructor(
    private val sink: HostLogSink,
) : Timber.Tree() {
    override fun isLoggable(tag: String?, priority: Int): Boolean =
        priority >= Log.INFO && tag?.startsWith(TAG_PREFIX) == true

    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
        sink.log(priority, tag.orEmpty(), message, emptyMap())
    }

    private companion object {
        const val TAG_PREFIX = "truapi."
    }
}
