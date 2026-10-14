package io.paritytech.polkadotapp.feature_chats_impl.data.worker

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import javax.inject.Inject

class ChatRequestRenewalScheduler @Inject constructor(
    @param:ApplicationContext private val appContext: Context,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        ChatRequestRenewalWorker.schedule(appContext)
    }
}
