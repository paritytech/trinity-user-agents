package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Test
import uniffi.truapi.WorkerBundle

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIWorkerSupervisorTest {
    @Test
    fun `engine boot failure goes to core with the exact execution`() = runTest {
        mockkStatic(Uri::class)
        try {
            every { Uri.parse("https://worker.paseo/worker.js") } returns mockk {
                every { scheme } returns "https"
                every { host } returns "worker.paseo"
                every { port } returns -1
                every { path } returns "/worker.js"
            }
            val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)
            val provider = mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) }
            val webViews = mockk<ChatWebViewProvider.Factory> {
                every { create(any(), any()) } throws IllegalStateException("engine unavailable")
            }
            val supervisor = TrUAPIWorkerSupervisor(
                runtimeProvider = { provider },
                bundles = mockk(),
                webViewProviderFactory = webViews,
                bootstrapInstaller = mockk(),
                pocketStore = mockk(),
                dispatchers = testDispatchers(),
            )
            val execution = mockk<TrUAPIProductExecution>()
            supervisor.startWorker("worker.paseo", execution, WorkerBundle(byteArrayOf(1), """{"scriptUrl":"https://worker.paseo/worker.js"}""".encodeToByteArray(), "/bundle"))
            advanceUntilIdle()
            verify(exactly = 1) { runtime.notifyWorkerFailed(execution, "engine unavailable") }
            verify(exactly = 1) { webViews.create(any(), any()) }
            supervisor.stopWorker("worker.paseo")
        } finally {
            unmockkStatic(Uri::class)
        }
    }
}
