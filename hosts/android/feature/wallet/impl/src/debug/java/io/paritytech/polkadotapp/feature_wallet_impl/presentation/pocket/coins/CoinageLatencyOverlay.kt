package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Color
import android.graphics.Typeface
import android.os.Build
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView

/**
 * Shows [CoinageLatencyProbe]'s numbers over the screen while the coins are attached. On the activity's
 * content rather than in the strip, which is too short for it and draws the coins over it.
 */
object CoinageLatencyOverlay {
    private const val BACKGROUND = 0xCC000000.toInt()
    private const val TEXT_SIZE = 10f

    fun attach(view: CoinageCoinsView) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return

        val overlay = TextView(view.context).apply {
            typeface = Typeface.MONOSPACE
            setTextSize(TypedValue.COMPLEX_UNIT_SP, TEXT_SIZE)
            setTextColor(Color.WHITE)
            setBackgroundColor(BACKGROUND)
        }
        val probe = CoinageLatencyProbe { overlay.text = it }

        view.frameTimings = probe
        view.addOnAttachStateChangeListener(object : View.OnAttachStateChangeListener {
            override fun onViewAttachedToWindow(attached: View) {
                attached.context.activity()?.findViewById<FrameLayout>(android.R.id.content)?.addView(
                    overlay,
                    FrameLayout.LayoutParams(
                        FrameLayout.LayoutParams.WRAP_CONTENT,
                        FrameLayout.LayoutParams.WRAP_CONTENT,
                        Gravity.CENTER_VERTICAL or Gravity.END
                    )
                )
                probe.start()
            }

            override fun onViewDetachedFromWindow(detached: View) {
                probe.stop()
                (overlay.parent as? FrameLayout)?.removeView(overlay)
            }
        })
    }

    private tailrec fun Context.activity(): Activity? = when (this) {
        is Activity -> this
        is ContextWrapper -> baseContext.activity()
        else -> null
    }
}
