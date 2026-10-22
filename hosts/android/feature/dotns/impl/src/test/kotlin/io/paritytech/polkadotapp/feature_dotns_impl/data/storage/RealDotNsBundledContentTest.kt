package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import kotlinx.coroutines.test.runTest
import org.junit.Test

class RealDotNsBundledContentTest {
    private val files = mapOf("index.js" to byteArrayOf(1), "manifest.json" to byteArrayOf(2))
    private val stored = mutableSetOf<String>()
    private val overrides = mutableMapOf<String, String>()
    private val storage: DotNsContentStorage = mockk {
        every { contentExists(any()) } answers { firstArg<String>() in stored }
        coEvery { saveContent(any(), any()) } answers { stored += firstArg<String>() }
    }
    private val contentHashOverrides: ContentHashOverrides = mockk {
        every { getContentHashOverride(any()) } answers { overrides[firstArg()] }
        every { putContentHashOverride(any(), any()) } answers { overrides[firstArg()] = secondArg() }
    }
    private val bundled = RealDotNsBundledContent(storage, contentHashOverrides)

    // Installed on every launch, so the archive is written once and the override is not rewritten.
    @Test
    fun `bundled content is stored once and served for its name`() = runTest {
        bundled.install("worker.chat.paseo", "aa", files)
        bundled.install("worker.chat.paseo", "aa", files)

        coVerify(exactly = 1) { storage.saveContent("aa", files) }
        verify(exactly = 1) { contentHashOverrides.putContentHashOverride("worker.chat.paseo", "aa") }
    }

    // Clearing the dotNS cache deletes the files but keeps the override, which would then send the app
    // to download a hash that exists nowhere. The next install has to put the files back.
    @Test
    fun `content removed by a cache clear is stored again`() = runTest {
        bundled.install("worker.chat.paseo", "aa", files)
        stored.clear()

        bundled.install("worker.chat.paseo", "aa", files)

        coVerify(exactly = 2) { storage.saveContent("aa", files) }
    }
}
