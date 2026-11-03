package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import android.net.Uri
import coil.fetch.SourceResult
import coil.request.CachePolicy
import coil.request.Options
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.publicProfileAvatarUri
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.ImageException
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import uniffi.truapi.LoadedImage

private val WEBP_BYTES = byteArrayOf(0x52, 0x49, 0x46, 0x46)

class PublicProfileAvatarFetcherTest {
    private val runtime: TrUAPIHostRuntime = mockk()
    private val runtimeProvider: TrUAPIHostRuntimeProvider = mockk {
        coEvery { runtime() } returns Result.success(runtime)
    }
    private val options: Options = mockk {
        every { context } returns mockk()
        every { diskCachePolicy } returns CachePolicy.ENABLED
    }
    private val factory = PublicProfileAvatarFetcher.Factory(runtimeProvider)

    // The core verifies the bytes against the record's CID; its detected format selects Coil's decoder.
    @Test
    fun `a contact's avatar is the image the core resolves for their username`() = runTest {
        coEvery { runtime.loadImage(ImageReference.Username("alice.01", force = false)) } returns
            LoadedImage(WEBP_BYTES, ImageFormat.WEBP, "avatar-cid")

        val result = factory.create(uriOf(publicProfileAvatarUri("alice.01")), options, mockk())?.fetch() as SourceResult

        assertEquals("image/webp", result.mimeType)
        assertArrayEquals(WEBP_BYTES, result.source.source().readByteArray())
    }

    // An opened chat refreshes its avatar past Coil's caches, so the core's cached record must not answer either.
    @Test
    fun `a refresh that skips the disk cache rereads the profile`() = runTest {
        every { options.diskCachePolicy } returns CachePolicy.WRITE_ONLY
        coEvery { runtime.loadImage(ImageReference.Username("alice.01", force = true)) } returns
            LoadedImage(WEBP_BYTES, ImageFormat.WEBP, "avatar-cid")

        val result = factory.create(uriOf(publicProfileAvatarUri("alice.01")), options, mockk())?.fetch() as SourceResult

        assertArrayEquals(WEBP_BYTES, result.source.source().readByteArray())
    }

    // No fetch result means Coil draws the error image, which for an avatar is the contact's letter.
    @Test
    fun `a username without an avatar leaves the letter drawn`() = runTest {
        coEvery { runtime.loadImage(ImageReference.Username("bob.02", force = false)) } throws ImageException.Unavailable("no avatar")

        assertNull(factory.create(uriOf(publicProfileAvatarUri("bob.02")), options, mockk())?.fetch())
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Username("bob.02", force = false)) }
    }

    @Test
    fun `other images are left to the fetchers that serve them`() {
        assertNull(factory.create(uriOf("https://example.com/icon.png"), options, mockk()))
    }

    // android.net.Uri has no JVM implementation in unit tests; this splits an opaque URI the way it does.
    private fun uriOf(value: String): Uri = mockk {
        every { scheme } returns value.substringBefore(':')
        every { schemeSpecificPart } returns value.substringAfter(':')
    }
}
