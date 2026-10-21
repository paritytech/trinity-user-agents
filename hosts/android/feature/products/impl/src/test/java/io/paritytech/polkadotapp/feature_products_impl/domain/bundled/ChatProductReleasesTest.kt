package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import android.content.Context
import android.content.SharedPreferences
import com.google.gson.Gson
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsVerifiedContent
import io.paritytech.polkadotapp.feature_dotns_api.domain.VerifiedContent
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.io.IOException

class ChatProductReleasesTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val tld = requireNotNull(DotNsTld.parse("paseo"))
    private val host = ProductManifest.hostOf(ReservedProductIds.chat(tld), ExecutableKind.WORKER)
    private val manifestParser = ManifestParser(Gson())

    private val stored = mutableMapOf<String, String>()
    private val preferences = mockk<SharedPreferences> {
        every { getString(any(), any()) } answers { stored[firstArg()] ?: secondArg() }
        every { edit() } returns mockk {
            every { putString(any(), any()) } answers { stored[firstArg()] = secondArg(); self as SharedPreferences.Editor }
            every { apply() } returns Unit
        }
    }
    private val context = mockk<Context> { every { getSharedPreferences(any(), any()) } returns preferences }

    private val bundledWorker = worker("[0,1,0]")
    private val bundled = mockk<BundledChatProduct> {
        coEvery { install(tld) } returns Unit
        coEvery { worker(tld) } returns bundledWorker
        every { resolved(tld, any()) } answers {
            ResolvedProduct(Product(ReservedProductIds.chat(tld), "Chats", icon = null), Executables(null, null, secondArg()))
        }
    }
    private val dotNsResolver = mockk<DotNsResolver>()
    private val verifiedContent = mockk<DotNsVerifiedContent>(relaxUnitFun = true)

    private fun releases() = ChatProductReleases(context, bundled, dotNsResolver, verifiedContent, manifestParser)

    private fun record(version: String) =
        """{"${'$'}v":1,"kind":"worker","appVersion":$version,"entrypoint":"index.js","includes":{"chat":true,"pocket":false},"keepAlive":"foreground"}"""

    private fun worker(version: String) =
        manifestParser.parseExecutable(record(version), ExecutableKind.WORKER, host).getOrThrow() as ProductExecutable.Worker

    private fun content(hash: String, withEntry: Boolean = true): VerifiedContent {
        val directory = folder.newFolder(hash)
        if (withEntry) File(directory, "index.js").writeText("worker")
        return VerifiedContent(hash, directory)
    }

    private fun publishes(version: String) {
        coEvery { dotNsResolver.getMetadataEntry(host.value, ProductManifest.EXECUTABLE_RECORD_KEY) } returns
            Result.success(record(version))
    }

    @Test
    fun `a newer release that passes its check runs, served from its own contenthash`() = runTest {
        publishes("[0,2,0]")
        coEvery { verifiedContent.fetch(host.value) } returns Result.success(content("aa"))

        val resolved = releases().resolved(tld)

        assertEquals(SemVer.fromComponents(0, 2, 0, null), resolved.executables.worker?.appVersion)
        verify { verifiedContent.serve(host.value, "aa") }
    }

    @Test
    fun `a release no newer than the bundled one is never fetched`() = runTest {
        publishes("[0,1,0]")

        assertEquals(bundledWorker, releases().resolved(tld).executables.worker)
        coVerify(exactly = 0) { verifiedContent.fetch(any()) }
    }

    @Test
    fun `a release without the worker's entry file does not run`() = runTest {
        publishes("[0,2,0]")
        coEvery { verifiedContent.fetch(host.value) } returns Result.success(content("bb", withEntry = false))

        assertEquals(bundledWorker, releases().resolved(tld).executables.worker)
        verify(exactly = 0) { verifiedContent.serve(any(), any()) }
    }

    @Test
    fun `while a newer release fails, the last release that passed keeps running`() = runTest {
        publishes("[0,2,0]")
        val passed = content("cc")
        coEvery { verifiedContent.fetch(host.value) } returns Result.success(passed)
        releases().resolved(tld)

        publishes("[0,3,0]")
        coEvery { verifiedContent.fetch(host.value) } returns Result.failure(IOException("tampered archive"))
        every { verifiedContent.kept("cc") } returns passed

        assertEquals(SemVer.fromComponents(0, 2, 0, null), releases().resolved(tld).executables.worker?.appVersion)
        verify(exactly = 2) { verifiedContent.serve(host.value, "cc") }
    }

    @Test
    fun `with dotNS unreadable and nothing that passed before, the bundled worker runs`() = runTest {
        coEvery { dotNsResolver.getMetadataEntry(host.value, ProductManifest.EXECUTABLE_RECORD_KEY) } returns
            Result.failure(IOException("offline"))

        assertEquals(bundledWorker, releases().resolved(tld).executables.worker)
    }
}
