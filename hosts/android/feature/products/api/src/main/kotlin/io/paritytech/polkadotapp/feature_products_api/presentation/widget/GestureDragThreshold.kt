package io.paritytech.polkadotapp.feature_products_api.presentation.widget

/**
 * How far a finger must move before a gesture node's pan starts.
 *
 * The system touch slop is 8 dp (about 1.3 mm on a Pixel 9). The existing slider reacts after about 1 mm, so the node
 * uses a lower threshold. The `debug.gesture.threshold` property, read once when the app starts, picks it, so one
 * build can run both: unset → [DEFAULT_DP]; a number → that many dp; `system` → the system touch slop, as before.
 * (`adb shell setprop debug.gesture.threshold system`, then restart the app.)
 */
internal object GestureDragThreshold {
    const val DEFAULT_DP = 5f
    private const val PROPERTY = "debug.gesture.threshold"
    private const val SYSTEM = "system"

    private val setting: String? = runCatching {
        Class.forName("android.os.SystemProperties").getMethod("get", String::class.java)
            .invoke(null, PROPERTY) as? String
    }.getOrNull()

    /** The threshold in px for a setting: the system's [systemSlopPx] for `system`, else dp scaled by [density]. */
    fun slopPx(setting: String?, systemSlopPx: Float, density: Float): Float {
        if (setting.isNullOrEmpty()) return DEFAULT_DP * density
        if (setting == SYSTEM) return systemSlopPx
        val dp = setting.toFloatOrNull()?.takeIf { it >= 0f } ?: DEFAULT_DP
        return dp * density
    }

    /** The threshold this run uses. */
    fun currentPx(systemSlopPx: Float, density: Float): Float = slopPx(setting, systemSlopPx, density)
}
