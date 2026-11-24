package io.parity.qjsbench

import android.app.Activity
import android.content.Context
import android.os.Build
import android.os.Bundle
import android.os.Debug
import android.os.PowerManager
import android.util.Log
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.io.File
import kotlin.system.exitProcess

/**
 * Runs the bench suite on launch, the Android counterpart of the iOS QJSBench.
 *
 * Extras: `suite` (`quick` or `full`), `runs` (replays per engine, default 3),
 * `exit` (quit when done). Every result is logged as `BENCH {json}` under the
 * `QJSBench` tag and appended to `files/results.jsonl`. Before a WebView
 * instances measurement is torn down, `BENCH_HOLD` is logged and the views are
 * held for a few seconds, so the driver can read the renderer process's memory.
 */
class MainActivity : Activity() {
    lateinit var webViewHost: FrameLayout
    private lateinit var output: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        webViewHost = FrameLayout(this)
        output = TextView(this).apply { textSize = 10f }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(webViewHost, LinearLayout.LayoutParams(1, 1))
            addView(ScrollView(this@MainActivity).apply { addView(output) })
        })

        val suite = intent.getStringExtra("suite") ?: "quick"
        val runs = intent.getIntExtra("runs", 3)
        val exitWhenDone = intent.getBooleanExtra("exit", false)
        Thread({ runSuite(suite, runs, exitWhenDone) }, "bench-driver").start()
    }

    private fun runSuite(suite: String, runs: Int, exitWhenDone: Boolean) {
        val resources = copyAssets()
        File(filesDir, "results.jsonl").delete()
        emit("{\"suite\":\"$suite\",\"variant\":\"${BenchKit.variant()}\",${deviceFields()}}")

        val engines = listOf("native", "wasm2c", "boa", "wasmi")
        val steps = mutableListOf<Pair<String, String>>("wasm2c" to "guard-capacity")
        for (engine in engines) {
            repeat(runs) { steps += engine to "replay" }
            steps += engine to "instances"
        }
        repeat(runs) { steps += "webview" to "replay" }
        steps += "webview" to "instances"
        steps += listOf("wasm2c" to "memcap", "wasmi" to "memcap", "wasm2c" to "oob-trap")
        val workloads = if (suite == "full") listOf("codec", "rxjs", "pingpong-64", "pingpong-4k") else listOf("codec")
        val throughputEngines = if (suite == "full") engines + "webview" else listOf("native", "wasm2c", "boa", "webview")
        for (workload in workloads) for (engine in throughputEngines) steps += engine to workload

        steps.forEachIndexed { index, (engine, mode) ->
            status("${index + 1}/${steps.size} $engine $mode")
            val line = if (engine == "webview") webView(mode, resources) else {
                val count = when (mode) { "instances" -> 10; "guard-capacity" -> 64; else -> 0 }
                BenchKit.run(engine, mode, resources.path, count)
            }
            emit("{${deviceFields()},\"step\":$index," + line.removePrefix("{"))
        }
        emit("{\"done\":true,\"steps\":${steps.size}}")
        status("Done, ${steps.size} steps")
        if (exitWhenDone) {
            Thread.sleep(300)
            finishAndRemoveTask()
            exitProcess(0)
        }
    }

    private fun webView(mode: String, resources: File): String {
        val head = "\"engine\":\"webview\",\"mode\":\"$mode\",\"variant\":\"android-webview\""
        val worker = File(resources, "w-worker.js").readText()
        return when (mode) {
            "replay" -> {
                val bench = WebViewBench(this)
                val bootMs = bench.boot(worker)
                val renderMs = mutableListOf<Double>()
                var mismatches = 0
                val steps = loadCorpus(File(resources, "corpus.bin"))
                for (step in steps) {
                    val started = System.nanoTime()
                    val sent = bench.deliver(step.inbound, step.outbound.size)
                    val elapsed = (System.nanoTime() - started) / 1e6
                    if (sent.size != step.outbound.size || sent.zip(step.outbound).any { (a, b) -> !a.contentEquals(b) }) mismatches++
                    if (isRenderStart(step.inbound)) renderMs += elapsed
                }
                bench.destroy()
                val (p50, p95) = percentiles(renderMs)
                "{$head,\"ok\":true,\"result\":{\"engine\":\"webview\",\"bootMs\":${"%.1f".format(bootMs)},\"steps\":${steps.size},\"mismatches\":$mismatches,\"renderSamples\":${renderMs.size},\"renderP50Ms\":${"%.3f".format(p50)},\"renderP95Ms\":${"%.3f".format(p95)}}}"
            }
            "instances" -> {
                val before = Debug.getPss()
                val benches = List(10) { WebViewBench(this) }
                val bootMs = benches.map { it.boot(worker) }
                Thread.sleep(500)
                val growthMiB = (Debug.getPss() - before) / 1024.0
                Log.i(TAG, "BENCH_HOLD webview-instances")
                Thread.sleep(8000)
                benches.forEach { it.destroy() }
                val (warmP50, _) = percentiles(bootMs.drop(1))
                "{$head,\"ok\":true,\"result\":{\"engine\":\"webview\",\"firstBootMs\":${"%.1f".format(bootMs[0])},\"bootP50Ms\":${"%.1f".format(warmP50)},\"appProcessGrowthMiB\":${"%.2f".format(growthMiB)},\"instances\":10}}"
            }
            else -> {
                val workload = File(resources, "w-$mode.js").readText()
                val bench = WebViewBench(this)
                val result = bench.runToResult(workload)
                bench.destroy()
                if (result != null) "{$head,\"ok\":true,\"result\":$result}" else "{$head,\"ok\":false,\"error\":\"no BENCH_RESULT\"}"
            }
        }
    }

    /** The workloads ship as assets; the engines read them from files. */
    private fun copyAssets(): File {
        val target = File(filesDir, "bench").apply { mkdirs() }
        for (name in assets.list("bench").orEmpty()) {
            assets.open("bench/$name").use { input -> File(target, name).outputStream().use { input.copyTo(it) } }
        }
        return target
    }

    private fun deviceFields(): String {
        val thermal = (getSystemService(Context.POWER_SERVICE) as PowerManager).currentThermalStatus
        return "\"model\":\"${Build.MODEL}\",\"android\":\"${Build.VERSION.RELEASE}\",\"thermal\":$thermal"
    }

    private fun emit(line: String) {
        Log.i(TAG, "BENCH $line")
        File(filesDir, "results.jsonl").appendText(line + "\n")
        runOnUiThread { output.append(line + "\n\n") }
    }

    private fun status(text: String) = runOnUiThread { title = text }

    private companion object {
        const val TAG = "QJSBench"
    }
}
