package io.paritytech.polkadotapp.feature_products_impl.isolation

import android.content.Context
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.google.gson.Gson
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.JsEngineState
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.webView.WebViewJsEngine
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class IsolationProbeTest {

    private lateinit var engine: WebViewJsEngine
    private lateinit var context: Context
    private val gson = Gson()

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        val guard = object : ProductPermissionGuard {
            override suspend fun requestPermission(productId: ProductId, permission: ProductPermission) = true
            override suspend fun check(productId: ProductId, permission: ProductPermission) = true
        }
        engine = WebViewJsEngine(context, gson, guard, RealCoroutineDispatchers())
    }

    @After
    fun tearDown() {
        runBlocking { engine.destroy() }
    }

    @Test
    fun isolationProbe() = runBlocking {
        engine.initialize(ProductId("isolation-test"))
        engine.state.first { it is JsEngineState.Ready }

        // Load container script (includes isolation overrides)
        val containerJs = context.assets.open("container.js").bufferedReader().readText()
        engine.evaluate(containerJs)

        // Load probe script
        val probeJs = context.assets.open("isolation-probe.js").bufferedReader().readText()
        engine.evaluate(probeJs)

        // Wait for probes to complete
        val resultsJson = withTimeout(30_000) {
            pollForResults()
        }

        Log.d("IsolationProbe", resultsJson)

        val summary = gson.fromJson(resultsJson, ProbeSummary::class.java)
        assertNotNull("Probe results must not be null", summary)
        assertNotNull("Probe results list must not be null", summary.results)

        val failures = summary.results.filter { !it.passed }
        if (failures.isNotEmpty()) {
            val message = buildString {
                appendLine("${failures.size} isolation probe(s) failed:")
                for (f in failures) {
                    appendLine("  [${f.id}] ${f.name} — expected: ${f.expected}, actual: ${f.actual}, error: ${f.error}")
                }
            }
            fail(message)
        }

        assertTrue("All ${summary.total} probes should pass", summary.passed == summary.total)
    }

    private suspend fun pollForResults(): String {
        while (true) {
            val result = engine.evaluate(
                "(function() { return window.__probe_results__ ? JSON.stringify(window.__probe_results__) : null; })()"
            ).getOrNull()

            if (!result.isNullOrEmpty() && result != "null") {
                return result
            }

            delay(500)
        }
    }
}

private data class ProbeSummary(
    val total: Int,
    val passed: Int,
    val failed: Int,
    val results: List<ProbeResult>,
)

private data class ProbeResult(
    val id: String,
    val category: String,
    val name: String,
    val expected: String,
    val actual: String,
    val passed: Boolean,
    val error: String?,
    val duration: Long?,
)
