package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RemoteFaceSource
import io.paritytech.polkadotapp.feature_products_impl.domain.pocketFacePreview.PocketFacePreviewInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import uniffi.truapi.RendererNode

private const val FACE_URL = "http://127.0.0.1:5173/pocket/devicehood.json"

class PocketFacePreviewViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val fetched = mutableListOf<String>()
    private var answer: Result<RendererNode> = Result.success(RendererNode.String("drawn"))
    private val remoteFaces = object : RemoteFaceSource {
        override suspend fun fetch(url: String): Result<RendererNode> {
            fetched += url
            return answer
        }
    }

    private val router: ProductsRouter = mock()

    private fun viewModel() = PocketFacePreviewViewModel(
        PocketFacePreviewInteractor(remoteFaces, mock(IpfsContentLookup::class.java)),
        router,
    )

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    /** The screen opens on its instructions, not on a spinner for a fetch nobody asked for. */
    @Test
    fun `nothing is fetched until a face is asked for`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()

        assertNull(model.state.value.face)
        assertEquals(emptyList<String>(), fetched)
    }

    /**
     * The whole loop is: edit the file, press Draw again. Without the attempt counter the request
     * flow holds the same value, emits nothing, and the second press silently redraws the face the
     * screen already had — which reads as the edit not having been saved.
     */
    @Test
    fun `drawing the same url twice fetches it twice`() = runTest(testDispatcher) {
        val model = viewModel()
        model.onUrlChanged(FACE_URL)

        model.onDrawClick()
        advanceUntilIdle()
        model.onDrawClick()
        advanceUntilIdle()

        assertEquals(listOf(FACE_URL, FACE_URL), fetched)
    }

    /**
     * Editing only an image leaves the face file as it was. The screen must still redraw on Draw, or its
     * images are never asked for again and the edit looks lost.
     */
    @Test
    fun `drawing an unchanged face again still redraws it`() = runTest(testDispatcher) {
        val model = viewModel()
        model.onUrlChanged(FACE_URL)

        model.onDrawClick()
        advanceUntilIdle()
        val first = model.state.value
        model.onDrawClick()
        advanceUntilIdle()
        val second = model.state.value

        assertEquals(first.face, second.face)
        assertNotEquals("the screen only redraws when its state changes", first, second)
    }

    @Test
    fun `a drawn face is what the screen shows`() = runTest(testDispatcher) {
        val model = viewModel()
        model.onUrlChanged(FACE_URL)

        model.onDrawClick()
        advanceUntilIdle()

        assertEquals(LoadingState.Loaded(JsWidget.Text(text = "drawn")), model.state.value.face)
    }

    /**
     * The failure is the useful half of this screen: a face is usually wrong before it is right, and
     * the cause has to reach the person editing it rather than being swallowed into an empty frame.
     */
    @Test
    fun `a failure reaches the screen as an error rather than an empty frame`() = runTest(testDispatcher) {
        answer = Result.failure(IllegalArgumentException("renderer node missing 'end'"))
        val model = viewModel()
        model.onUrlChanged(FACE_URL)

        model.onDrawClick()
        advanceUntilIdle()

        val face = model.state.value.face
        assertTrue("a refused face must not read as nothing drawn yet", face is LoadingState.Error)
        assertEquals("renderer node missing 'end'", (face as LoadingState.Error).exception.message)
    }

    /** Draw is disabled on a blank url; nothing should reach the network if it is pressed anyway. */
    @Test
    fun `a blank url is not fetched`() = runTest(testDispatcher) {
        val model = viewModel()
        model.onUrlChanged("   ")

        model.onDrawClick()
        advanceUntilIdle()

        assertEquals(emptyList<String>(), fetched)
        assertNull(model.state.value.face)
    }

    /** The url is what the person typed; the fetch is what the server can answer. */
    @Test
    fun `surrounding whitespace is trimmed before the url is fetched`() = runTest(testDispatcher) {
        val model = viewModel()
        model.onUrlChanged("  $FACE_URL  ")

        model.onDrawClick()
        advanceUntilIdle()

        assertEquals(listOf(FACE_URL), fetched)
    }

    /** Images belong to the face on screen. Typing the next url must not move them before it is drawn. */
    @Test
    fun `images come from the face that was drawn, not the url being typed`() = runTest(testDispatcher) {
        val model = viewModel()
        model.onUrlChanged(FACE_URL)
        model.onDrawClick()
        advanceUntilIdle()

        model.onUrlChanged("http://127.0.0.1:9999/pocket/other.json")

        assertEquals(
            "http://127.0.0.1:5173/images/stamp.png",
            model.imageResolver.resolve(JsImageSource.Archive("images/stamp.png")),
        )
    }
}
