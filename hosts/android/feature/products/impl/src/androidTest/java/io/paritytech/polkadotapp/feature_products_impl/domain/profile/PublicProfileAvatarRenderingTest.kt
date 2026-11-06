package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import coil.Coil
import coil.EventListener
import coil.ImageLoader
import coil.disk.DiskCache
import coil.request.ErrorResult
import coil.request.ImageRequest
import coil.request.SuccessResult
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.design.components.avatar.AVATAR_REFRESH_PARAMETER
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.publicProfileAvatarUri
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import uniffi.truapi.LoadedImage
import uniffi.truapi.ProfileAvatar
import uniffi.truapi.PublicProfile
import java.io.ByteArrayOutputStream
import java.util.concurrent.atomic.AtomicInteger
import android.graphics.Color as AndroidColor

@RunWith(AndroidJUnit4::class)
class PublicProfileAvatarRenderingTest {
    @get:Rule
    val compose = createComposeRule()

    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private val runtime: TrUAPIHostRuntime = mockk()
    private val runtimeProvider: TrUAPIHostRuntimeProvider = mockk {
        coEvery { runtime() } returns Result.success(runtime)
    }
    private val avatar = ProfileAvatar("alice-cid", ImageFormat.PNG)
    private val nextAvatar = ProfileAvatar("next-cid", ImageFormat.PNG)
    private val username = mutableStateOf("alice.01")
    private val placeholder = AvatarUiModel.Name("Alice", AvatarColorScheme.from("alice".encodeToByteArray()))
    private val refreshesCompleted = AtomicInteger()
    private lateinit var imageLoader: ImageLoader

    @Before
    fun setUp() {
        imageLoader = ImageLoader.Builder(context)
            .allowHardware(false)
            .diskCache(DiskCache.Builder().directory(temporaryFolder.newFolder()).maxSizeBytes(1_024 * 1_024).build())
            .components { PublicProfileAvatarImageLoaderRegistrar(runtimeProvider).register(this) }
            .eventListener(object : EventListener {
                override fun onSuccess(request: ImageRequest, result: SuccessResult) = completed(request)
                override fun onError(request: ImageRequest, result: ErrorResult) = completed(request)
            })
            .build()
        Coil.setImageLoader(imageLoader)
    }

    @After
    fun tearDown() {
        imageLoader.shutdown()
    }

    @Test
    fun cachedAvatarRemainsVisibleWhileMetadataRefreshIsPending() {
        withCachedAvatar()
        val refresh = withPendingRefresh()

        drawAvatar()
        compose.waitUntil(TIMEOUT_MILLIS) { refresh.started.isCompleted }

        assertEquals(Color.Red, avatarColor())
        refresh.result.complete(PublicProfile(avatar))
        compose.waitUntil(TIMEOUT_MILLIS) { refreshesCompleted.get() == 1 }
        assertEquals(Color.Red, avatarColor())
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Bulletin(avatar.cid, avatar.format)) }
    }

    @Test
    fun changedAvatarDownloadsTheNewCidAndUpdatesTheImage() {
        withCachedAvatar()
        withImage(nextAvatar, AndroidColor.BLUE)
        val refresh = withPendingRefresh()

        drawAvatar()
        compose.waitUntil(TIMEOUT_MILLIS) { refresh.started.isCompleted }
        assertEquals(Color.Red, avatarColor())
        refresh.result.complete(PublicProfile(nextAvatar))

        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Blue }
        coVerify(exactly = 1) { runtime.loadImage(ImageReference.Bulletin(nextAvatar.cid, nextAvatar.format)) }
    }

    @Test
    fun removedAvatarRestoresTheLetter() {
        withCachedAvatar()
        val refresh = withPendingRefresh()

        drawAvatar()
        compose.waitUntil(TIMEOUT_MILLIS) { refresh.started.isCompleted }
        assertEquals(Color.Red, avatarColor())
        refresh.result.complete(PublicProfile(null))

        compose.waitUntil(TIMEOUT_MILLIS) { refreshesCompleted.get() == 1 }
        assertEquals(interiorPixels("letter"), interiorPixels("avatar"))
    }

    @Test
    fun failedRefreshKeepsTheCachedImage() {
        withCachedAvatar()
        val refresh = withPendingRefresh()

        drawAvatar()
        compose.waitUntil(TIMEOUT_MILLIS) { refresh.started.isCompleted }
        refresh.result.completeExceptionally(IllegalStateException("offline"))

        compose.waitUntil(TIMEOUT_MILLIS) { refreshesCompleted.get() == 1 }
        assertEquals(Color.Red, avatarColor())
    }

    @Test
    fun changingUsernameCancelsTheOldRefreshAndCannotDisplayItsResult() {
        withCachedAvatar()
        val refresh = withPendingRefresh()
        withImage(nextAvatar, AndroidColor.BLUE)
        coEvery { runtime.resolvePublicProfile("bob.01", any()) } returns PublicProfile(nextAvatar)

        drawAvatar()
        compose.waitUntil(TIMEOUT_MILLIS) { refresh.started.isCompleted }
        compose.runOnIdle { username.value = "bob.01" }
        compose.waitUntil(TIMEOUT_MILLIS) { refresh.cancelled.isCompleted && avatarColor() == Color.Blue }
        refresh.result.complete(PublicProfile(null))

        compose.waitForIdle()
        assertEquals(Color.Blue, avatarColor())
    }

    private fun withCachedAvatar() {
        coEvery { runtime.resolvePublicProfile("alice.01", false) } returns PublicProfile(avatar)
        withImage(avatar, AndroidColor.RED)
        cacheImage(publicProfileAvatarUri("alice.01"))
    }

    private fun withImage(image: ProfileAvatar, color: Int) {
        val bytes = ByteArrayOutputStream().use { output ->
            Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888).apply {
                eraseColor(color)
                compress(Bitmap.CompressFormat.PNG, 100, output)
                recycle()
            }
            output.toByteArray()
        }
        coEvery { runtime.loadImage(ImageReference.Bulletin(image.cid, image.format)) } returns
            LoadedImage(bytes, image.format, image.cid)
    }

    private fun cacheImage(data: Any) {
        val result = runBlocking {
            imageLoader.execute(ImageRequest.Builder(context).data(data).size(64).build())
        }
        assertTrue("Cache warmup failed: $result", result is SuccessResult)
    }

    private fun withPendingRefresh(): PendingRefresh {
        val refresh = PendingRefresh()
        coEvery { runtime.resolvePublicProfile("alice.01", true) } coAnswers {
            refresh.started.complete(Unit)
            try {
                refresh.result.await()
            } catch (error: CancellationException) {
                refresh.cancelled.complete(Unit)
                throw error
            }
        }
        return refresh
    }

    private fun drawAvatar() {
        compose.setContent {
            PolkadotTheme(PolkadotAppTheme.BerlinNight) {
                Row {
                    PolkadotAvatar(
                        model = AvatarUiModel.Image(publicProfileAvatarUri(username.value), placeholder, refresh = true),
                        modifier = Modifier.size(48.dp).testTag("avatar"),
                    )
                    PolkadotAvatar(model = placeholder, modifier = Modifier.size(48.dp).testTag("letter"))
                }
            }
        }
    }

    private fun avatarColor(): Color {
        val pixels = compose.onNodeWithTag("avatar").captureToImage().toPixelMap()
        return pixels[pixels.width / 2, pixels.height / 2]
    }

    private fun interiorPixels(tag: String): List<Color> {
        val pixels = compose.onNodeWithTag(tag).captureToImage().toPixelMap()
        return (pixels.height / 4 until pixels.height * 3 / 4).flatMap { row ->
            (pixels.width / 4 until pixels.width * 3 / 4).map { column -> pixels[column, row] }
        }
    }

    private fun completed(request: ImageRequest) {
        if (request.parameters.value<Boolean>(AVATAR_REFRESH_PARAMETER) == true) refreshesCompleted.incrementAndGet()
    }

    private companion object {
        const val TIMEOUT_MILLIS = 10_000L
    }

    private class PendingRefresh {
        val started = CompletableDeferred<Unit>()
        val result = CompletableDeferred<PublicProfile>()
        val cancelled = CompletableDeferred<Unit>()
    }
}
