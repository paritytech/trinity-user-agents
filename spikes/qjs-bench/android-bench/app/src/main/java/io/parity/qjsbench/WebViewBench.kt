package io.parity.qjsbench

import android.annotation.SuppressLint
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * The same workload in an Android WebView, as the host runs workers today:
 * frames leave through a JavaScript interface and come back through
 * `__truapi_deliver_bytes`, as in the Mac WKWebView bench. Every call blocks
 * the bench thread and runs the WebView on the main thread.
 */
@SuppressLint("SetJavaScriptEnabled")
class WebViewBench(private val activity: MainActivity) {
    private val main = Handler(Looper.getMainLooper())
    private lateinit var webView: WebView
    private val frames = mutableListOf<ByteArray>()
    private var framesWanted = 0
    private var framesArrived: CountDownLatch? = null
    private var result: String? = null
    private var resultArrived = CountDownLatch(1)

    private inner class Bridge {
        @JavascriptInterface
        fun frame(bytes: String) {
            val frame = if (bytes.isEmpty()) ByteArray(0) else
                bytes.split(',').map { it.toInt().toByte() }.toByteArray()
            synchronized(frames) {
                frames.add(frame)
                if (frames.size >= framesWanted) framesArrived?.countDown()
            }
        }

        @JavascriptInterface
        fun log(level: String, message: String) {
            if (message.startsWith("BENCH_RESULT ")) {
                result = message.removePrefix("BENCH_RESULT ")
                resultArrived.countDown()
            }
        }
    }

    /** Create the WebView, load a blank page, then the shim and the workload. */
    fun boot(workload: String): Double {
        val started = System.nanoTime()
        val loaded = CountDownLatch(1)
        onMain {
            webView = WebView(activity).apply {
                settings.javaScriptEnabled = true
                addJavascriptInterface(Bridge(), "truapiNative")
                webViewClient = object : WebViewClient() {
                    override fun onPageFinished(view: WebView, url: String) = loaded.countDown()
                }
            }
            // Attached and 1 px, so Chromium neither throttles nor pauses it as hidden.
            activity.webViewHost.addView(webView, FrameLayout.LayoutParams(1, 1))
            webView.loadDataWithBaseURL("https://bench.local/", "<!doctype html><html><body></body></html>", "text/html", "utf-8", null)
        }
        loaded.await(30, TimeUnit.SECONDS)
        evaluate(SHIM)
        evaluate(workload)
        evaluate("0")
        return (System.nanoTime() - started) / 1e6
    }

    fun evaluate(source: String) {
        val done = CountDownLatch(1)
        onMain { webView.evaluateJavascript(source) { done.countDown() } }
        done.await(60, TimeUnit.SECONDS)
    }

    /** Deliver one frame and wait until `expected` frames have come back. */
    fun deliver(frame: ByteArray, expected: Int): List<ByteArray> {
        val arrived = CountDownLatch(1)
        synchronized(frames) {
            frames.clear()
            framesWanted = expected
            framesArrived = arrived
            if (expected == 0) arrived.countDown()
        }
        val call = "__truapi_deliver_bytes([${frame.joinToString(",") { (it.toInt() and 0xff).toString() }}]);0"
        onMain { webView.evaluateJavascript(call, null) }
        arrived.await(5, TimeUnit.SECONDS)
        if (expected == 0) evaluate("0")
        return synchronized(frames) { frames.toList() }
    }

    /** Run a throughput workload to its BENCH_RESULT line. */
    fun runToResult(workload: String): String? {
        resultArrived = CountDownLatch(1)
        boot(workload)
        resultArrived.await(120, TimeUnit.SECONDS)
        return result
    }

    fun destroy() {
        val done = CountDownLatch(1)
        onMain {
            activity.webViewHost.removeView(webView)
            webView.destroy()
            done.countDown()
        }
        done.await(10, TimeUnit.SECONDS)
    }

    private fun onMain(block: () -> Unit) {
        main.post(block)
    }

    companion object {
        /** The Mac shim's frame channel, over `truapiNative` instead of WebKit's handlers. */
        private val SHIM = """
            (() => {
              const listeners = new Set();
              globalThis.__truapi_frames = {
                send: (frame) => truapiNative.frame(Array.from(frame).join(",")),
                subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
              };
              globalThis.__truapi_deliver_bytes = (bytes) => {
                const frame = Uint8Array.from(bytes);
                for (const listener of [...listeners]) listener(frame);
              };
              const forward = (level) => (...args) => truapiNative.log(level, args.map(String).join(" "));
              console.log = forward("info");
              console.warn = forward("warn");
              console.error = forward("error");
            })();
        """.trimIndent()
    }
}
