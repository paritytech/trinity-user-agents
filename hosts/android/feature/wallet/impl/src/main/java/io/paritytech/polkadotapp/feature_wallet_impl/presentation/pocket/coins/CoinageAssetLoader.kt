package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.content.Context
import io.paritytech.polkadotapp.common.utils.logFailure
import java.util.concurrent.Executors

/**
 * Decodes the one asset bundle, once, away from the main thread.
 *
 * Everything in it — the meshes, the studio, the relief atlas, the shader sources — is the same for every
 * coin on every screen, and none of it changes while the app runs. Decoding it in the view meant it happened
 * again on every appearance and, worse, with the main thread held: the studio alone is forty-two Radiance
 * faces. That is the whole of the pause between tapping the card and anything happening.
 *
 * Callers get the bundle when it is ready and draw nothing until then, which is a frame or two of an empty
 * strip rather than a frozen tap. GL objects are deliberately not shared: building them from a decoded
 * bundle is uploads only, and sharing them across contexts would mean sharing the contexts too.
 */
object CoinageAssetLoader {
    private val worker = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "coinage-assets").apply { isDaemon = true }
    }

    private val lock = Any()
    private var bundle: CoinageAssetBundle? = null
    private var isLoading = false
    private var waiting = mutableListOf<(CoinageAssetBundle?) -> Unit>()

    /**
     * Hands over the bundle, decoding it first if nobody has yet. The callback runs on the loader's own
     * thread, so a caller that needs another has to post.
     */
    fun load(context: Context, deliver: (CoinageAssetBundle?) -> Unit) {
        val ready = synchronized(lock) {
            val loaded = bundle

            when {
                loaded != null -> loaded
                isLoading -> {
                    waiting += deliver
                    return
                }

                else -> {
                    isLoading = true
                    waiting += deliver
                    null
                }
            }
        }

        if (ready != null) {
            deliver(ready)
            return
        }

        val assets = context.applicationContext.assets

        worker.execute {
            val decoded = runCatching { CoinageAssetStore.load(assets) }
                .logFailure("CoinageAssetLoader: could not decode the coin assets")
                .getOrNull()

            val deliveries = synchronized(lock) {
                bundle = decoded
                isLoading = false
                waiting.also { waiting = mutableListOf() }
            }

            for (callback in deliveries) callback(decoded)
        }
    }
}
