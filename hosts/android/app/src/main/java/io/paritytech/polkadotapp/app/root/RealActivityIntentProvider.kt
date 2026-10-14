package io.paritytech.polkadotapp.app.root

import android.content.Context
import android.content.Intent
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.app.root.presentation.root.RootActivity
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.feature_calls_impl.presentation.CallActivity
import javax.inject.Inject

class RealActivityIntentProvider @Inject constructor(
    @param:ApplicationContext private val context: Context
) : ActivityIntentProvider {
    override fun getRootIntent(): Intent {
        return Intent(context, RootActivity::class.java)
    }

    override fun getIncomingCallIntent(): Intent {
        return Intent(context, CallActivity::class.java)
    }
}
