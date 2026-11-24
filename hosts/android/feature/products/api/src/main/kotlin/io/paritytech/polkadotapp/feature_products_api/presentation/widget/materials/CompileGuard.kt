package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.content.SharedPreferences
import io.novasama.substrate_sdk_android.extensions.toHexString

/**
 * Marks each compile on disk. A marker still there at process start was left by a compile that took the
 * app down.
 */
class CompileGuard internal constructor(private val prefs: SharedPreferences) {
    private val leftOver: Set<String> by lazy { prefs.all.keys.toSet() }

    /** Whether compiling [material] took the app down in an earlier process. */
    fun crashed(material: ByteArray): Boolean = material.toHexString() in leftOver

    internal fun <T> compiling(material: ByteArray, block: () -> T): T {
        val key = material.toHexString()
        prefs.edit().putBoolean(key, true).commit()
        try {
            return block()
        } finally {
            prefs.edit().remove(key).commit()
        }
    }
}
