package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import android.content.SharedPreferences
import io.novasama.substrate_sdk_android.extensions.toHexString

// A marker still there at process start was left by a compile that took the app down.
internal class CompileGuard(private val prefs: SharedPreferences) {
    private val leftOver: Set<String> by lazy { prefs.all.keys.toSet() }

    fun crashed(material: ByteArray): Boolean = material.toHexString() in leftOver

    fun <T> compiling(material: ByteArray, block: () -> T): T {
        val key = material.toHexString()
        prefs.edit().putBoolean(key, true).commit()
        try {
            return block()
        } finally {
            prefs.edit().remove(key).commit()
        }
    }
}
