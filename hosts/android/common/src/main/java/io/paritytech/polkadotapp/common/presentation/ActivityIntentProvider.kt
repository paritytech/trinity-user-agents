package io.paritytech.polkadotapp.common.presentation

import android.content.Intent

interface ActivityIntentProvider {
    fun getRootIntent(): Intent
    fun getIncomingCallIntent(): Intent
}
