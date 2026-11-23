package io.paritytech.polkadotapp.app

import android.annotation.SuppressLint
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.WebViewProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class WebViewWorkerModuleTest {
    @Test
    fun aWorkerWaitingForTheHostCanReceiveItsFirstCall() = withWorkerPage { runtime ->
        runtime.startWorkerModule("data:text/javascript,window.reply = await new Promise(resolve => window.respond = resolve)")
        runtime.evaluate("window.respond('echo')").getOrThrow()

        assertEquals("echo", runtime.evaluate("window.reply").getOrThrow())
    }

    @Test
    fun aBrokenOrMissingModuleCannotBecomeReady() = withWorkerPage { runtime ->
        val missing = runtime.evaluate("""
            (() => { const url = URL.createObjectURL(new Blob()); URL.revokeObjectURL(url); return url })()
        """.trimIndent()).getOrThrow()

        for (url in listOf(missing, "data:text/javascript,invalid syntax")) {
            val failure = runCatching { runtime.startWorkerModule(url) }.exceptionOrNull()
            assertTrue("expected a module error for $url, got $failure", failure is IllegalStateException)
        }
    }

    private fun withWorkerPage(test: suspend (WebViewRuntime) -> Unit) = runBlocking {
        withContext(Dispatchers.Main) {
            val page = Page()
            val runtime = WebViewRuntime(page)
            try {
                withTimeout(10_000) {
                    runtime.initialize()
                    runtime.loadInitialPage()
                    runtime.waitForReady()
                    test(runtime)
                }
            } finally {
                runtime.dispose()
                assertTrue("shutdown must destroy the detached worker WebView", page.destroyed)
            }
        }
    }

    private class Page : WebViewProvider(RealCoroutineDispatchers()) {
        var destroyed = false

        override val callingProductIdProvider = CallingProductIdProvider { error("This page only loads test modules") }

        @SuppressLint("SetJavaScriptEnabled")
        override suspend fun createWebView(): WebView = object : WebView(InstrumentationRegistry.getInstrumentation().targetContext) {
            override fun destroy() {
                super.destroy()
                destroyed = true
            }
        }.apply {
            settings.javaScriptEnabled = true
            webViewClient = object : WebViewClient() {
                override fun onPageFinished(view: WebView, url: String) = notifyOnPageFinished()
            }
        }

        override suspend fun loadInitialContent() {
            accessWebView { it.loadDataWithBaseURL("https://worker.test/", "<html></html>", "text/html", "UTF-8", null) }
        }
    }
}
