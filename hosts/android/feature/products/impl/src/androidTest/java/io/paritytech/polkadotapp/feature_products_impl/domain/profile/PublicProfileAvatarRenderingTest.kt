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
import coil.request.ErrorResult
import coil.request.ImageRequest
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.avatar.PublicProfileAvatar
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.image.BulletinImageLoaderRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.ImageFormat
import uniffi.truapi.ImageReference
import uniffi.truapi.LoadedImage
import uniffi.truapi.ProfileAvatar
import uniffi.truapi.PublicProfile
import java.io.ByteArrayOutputStream
import android.graphics.Color as AndroidColor

@RunWith(AndroidJUnit4::class)
class PublicProfileAvatarRenderingTest {
    @get:Rule
    val compose = createComposeRule()

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private val runtime: TrUAPIHostRuntime = mockk()
    private val runtimeProvider: TrUAPIHostRuntimeProvider = mockk {
        coEvery { runtime() } returns Result.success(runtime)
    }
    private val avatar = ProfileAvatar("alice-cid", ImageFormat.PNG)
    private val nextAvatar = ProfileAvatar("next-cid", ImageFormat.PNG)
    private val bobAvatar = ProfileAvatar("bob-cid", ImageFormat.PNG)
    private val username = mutableStateOf("alice.01")
    private val shown = mutableStateOf(true)
    private val failedImages = Channel<ImageReference.Bulletin>(Channel.UNLIMITED)
    private val placeholder = AvatarUiModel.Name("Alice", AvatarColorScheme.from("alice".encodeToByteArray()))
    private lateinit var imageLoader: ImageLoader
    private var contentShown = false

    @Before
    fun setUp() {
        imageLoader = ImageLoader.Builder(context)
            .allowHardware(false)
            .diskCache(null)
            .components { BulletinImageLoaderRegistrar(runtimeProvider).register(this) }
            .eventListener(object : EventListener {
                override fun onError(request: ImageRequest, result: ErrorResult) {
                    (request.data as? ImageReference.Bulletin)?.let { failedImages.trySend(it) }
                }
            })
            .build()
        Coil.setImageLoader(imageLoader)
    }

    @After
    fun tearDown() {
        if (contentShown) {
            compose.runOnIdle { shown.value = false }
            compose.waitForIdle()
        }
        imageLoader.shutdown()
    }

    @Test
    fun replacementMetadataIsCollectedWhileTheFirstImageIsStillLoading() {
        val firstImage = withPendingImage(avatar, AndroidColor.RED)
        withImage(nextAvatar, AndroidColor.BLUE)
        val updates = withUpdates()
        drawAvatar()
        updates.emit(PublicProfile(avatar))
        compose.waitUntil(TIMEOUT_MILLIS) { firstImage.started.isCompleted }

        updates.emit(PublicProfile(nextAvatar))

        compose.waitUntil(TIMEOUT_MILLIS) { firstImage.cancelled.isCompleted && avatarColor() == Color.Blue }
        firstImage.release.complete(Unit)
        compose.waitForIdle()
        assertEquals(Color.Blue, avatarColor())
    }

    @Test
    fun currentImageRemainsVisibleUntilTheReplacementImageIsReady() {
        withImage(avatar, AndroidColor.RED)
        val replacement = withPendingImage(nextAvatar, AndroidColor.BLUE)
        val updates = withUpdates()
        drawAvatar()
        updates.emit(PublicProfile(avatar))
        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Red }

        updates.emit(PublicProfile(nextAvatar))
        compose.waitUntil(TIMEOUT_MILLIS) { replacement.started.isCompleted }
        assertEquals(Color.Red, avatarColor())
        replacement.release.complete(Unit)

        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Blue }
    }

    @Test
    fun removedAvatarRestoresTheLetter() {
        withImage(avatar, AndroidColor.RED)
        val updates = withUpdates()
        drawAvatar()
        updates.emit(PublicProfile(avatar))
        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Red }

        updates.emit(PublicProfile(null))

        compose.waitUntil(TIMEOUT_MILLIS) { interiorPixels("letter") == interiorPixels("avatar") }
        assertEquals(interiorPixels("letter"), interiorPixels("avatar"))
    }

    @Test
    fun failedMetadataRefreshKeepsTheCurrentImage() {
        withImage(avatar, AndroidColor.RED)
        val updates = withUpdates()
        drawAvatar()
        updates.emit(PublicProfile(avatar))
        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Red }

        updates.fail(IllegalStateException("offline"))

        compose.waitUntil(TIMEOUT_MILLIS) { updates.finished.isCompleted }
        compose.waitForIdle()
        assertEquals(Color.Red, avatarColor())
    }

    @Test
    fun failedReplacementImageKeepsTheCurrentImage() {
        withImage(avatar, AndroidColor.RED)
        val replacement = withPendingImage(nextAvatar, AndroidColor.BLUE)
        val updates = withUpdates()
        drawAvatar()
        updates.emit(PublicProfile(avatar))
        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Red }
        updates.emit(PublicProfile(nextAvatar))
        compose.waitUntil(TIMEOUT_MILLIS) { replacement.started.isCompleted }

        replacement.release.completeExceptionally(IllegalStateException("image unavailable"))

        var failure: ImageReference.Bulletin? = null
        compose.waitUntil(TIMEOUT_MILLIS) {
            failedImages.tryReceive().getOrNull()?.let { failure = it }
            failure != null
        }
        compose.waitForIdle()
        assertEquals(ImageReference.Bulletin(nextAvatar.cid, nextAvatar.format), failure)
        assertEquals(Color.Red, avatarColor())
    }

    @Test
    fun changingUsernameCancelsOldMetadataAndImageWork() {
        withImage(avatar, AndroidColor.RED)
        val obsoleteImage = withPendingImage(nextAvatar, AndroidColor.BLUE)
        val oldUpdates = withUpdates()
        val newUpdates = withUpdates("bob.01")
        val newImage = withPendingImage(bobAvatar, AndroidColor.GREEN)
        drawAvatar()
        oldUpdates.emit(PublicProfile(avatar))
        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Red }
        oldUpdates.emit(PublicProfile(nextAvatar))
        compose.waitUntil(TIMEOUT_MILLIS) { obsoleteImage.started.isCompleted }

        compose.runOnIdle { username.value = "bob.01" }
        newUpdates.emit(PublicProfile(bobAvatar))

        compose.waitUntil(TIMEOUT_MILLIS) {
            oldUpdates.cancelled.isCompleted && obsoleteImage.cancelled.isCompleted && newImage.started.isCompleted
        }
        assertEquals(interiorPixels("letter"), interiorPixels("avatar"))
        newImage.release.complete(Unit)
        compose.waitUntil(TIMEOUT_MILLIS) { avatarColor() == Color.Green }
        obsoleteImage.release.complete(Unit)
        compose.waitForIdle()
        assertEquals(Color.Green, avatarColor())
    }

    private fun withUpdates(name: String = "alice.01"): ProfileUpdates = ProfileUpdates().also { updates ->
        every { runtime.observePublicProfile(name, any()) } returns updates.flow
    }

    private fun withImage(image: ProfileAvatar, color: Int) {
        coEvery { runtime.loadImage(ImageReference.Bulletin(image.cid, image.format)) } returns loadedImage(image, color)
    }

    private fun withPendingImage(image: ProfileAvatar, color: Int): PendingImage = PendingImage().also { pending ->
        coEvery { runtime.loadImage(ImageReference.Bulletin(image.cid, image.format)) } coAnswers {
            pending.started.complete(Unit)
            try {
                pending.release.await()
                loadedImage(image, color)
            } catch (error: CancellationException) {
                pending.cancelled.complete(Unit)
                throw error
            }
        }
    }

    private fun loadedImage(image: ProfileAvatar, color: Int): LoadedImage {
        val bytes = ByteArrayOutputStream().use { output ->
            Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888).apply {
                eraseColor(color)
                compress(Bitmap.CompressFormat.PNG, 100, output)
                recycle()
            }
            output.toByteArray()
        }
        return LoadedImage(bytes, image.format, image.cid)
    }

    private fun drawAvatar() {
        val source = CorePublicProfileAvatarSource(context, runtimeProvider)
        compose.setContent {
            PolkadotTheme(PolkadotAppTheme.BerlinNight) {
                Row {
                    if (shown.value) {
                        PublicProfileAvatar(
                            model = AvatarUiModel.PublicProfile(username.value, placeholder, forceFresh = true),
                            source = source,
                            modifier = Modifier.size(48.dp).testTag("avatar"),
                        )
                    }
                    PolkadotAvatar(model = placeholder, modifier = Modifier.size(48.dp).testTag("letter"))
                }
            }
        }
        contentShown = true
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

    private companion object {
        const val TIMEOUT_MILLIS = 10_000L
    }

    private class PendingImage {
        val started = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val cancelled = CompletableDeferred<Unit>()
    }

    private class ProfileUpdates {
        private val values = Channel<PublicProfile>(Channel.UNLIMITED)
        val cancelled = CompletableDeferred<Unit>()
        val finished = CompletableDeferred<Unit>()
        val flow: Flow<PublicProfile> = flow {
            try {
                for (profile in values) emit(profile)
            } catch (error: CancellationException) {
                cancelled.complete(Unit)
                throw error
            } finally {
                finished.complete(Unit)
            }
        }

        fun emit(profile: PublicProfile) {
            values.trySend(profile).getOrThrow()
        }

        fun fail(error: Throwable) {
            values.close(error)
        }
    }
}
