package io.paritytech.polkadotapp.feature_coinage_impl.data.source

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import javax.inject.Inject

/**
 * Signals system clock changes so voucher readiness deadlines can be recalculated.
 */
class ClockChangesSource @Inject constructor(
    @param:ApplicationContext private val context: Context,
) {
    fun changes(): Flow<Unit> = callbackFlow {
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                trySend(Unit)
            }
        }
        ContextCompat.registerReceiver(context, receiver, IntentFilter(Intent.ACTION_TIME_CHANGED), ContextCompat.RECEIVER_NOT_EXPORTED)
        awaitClose { context.unregisterReceiver(receiver) }
    }
}
