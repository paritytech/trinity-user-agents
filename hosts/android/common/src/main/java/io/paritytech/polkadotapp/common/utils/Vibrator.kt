package io.paritytech.polkadotapp.common.utils

import android.annotation.SuppressLint
import android.os.VibrationEffect
import android.os.Vibrator
import kotlin.time.Duration

@SuppressLint("MissingPermission")
fun Vibrator.vibrate(duration: Duration) {
    vibrate(VibrationEffect.createOneShot(duration.inWholeMilliseconds, VibrationEffect.DEFAULT_AMPLITUDE))
}
