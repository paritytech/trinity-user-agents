package io.paritytech.polkadotapp.feature_connection_status_api.presentation

import android.graphics.Bitmap
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthItemModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.IndicatorRow
import kotlinx.collections.immutable.toImmutableList
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

// Renders the "Network Status" panel in every indicator state, inside the same rounded container the tab
// bar expands it into, and saves each as a PNG for design review.
@RunWith(AndroidJUnit4::class)
class ChainHealthPanelScreenshotTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun healthy() = render("healthy", share = 13f / 15f)

    @Test
    fun producingFourFifths() = render("production-80", share = 0.8f)

    @Test
    fun producingUnderHalf() = render("production-40", share = 0.4f)

    @Test
    fun producingUnderAQuarter() = render("production-20", share = 0.2f)

    @Test
    fun notProducingBlocks() = render("not-producing", share = 0f)

    @Test
    fun connecting() = render("connecting", ChainHealthIndicator.Connecting)

    @Test
    fun broken() = render("broken", ChainHealthIndicator.Disconnected)

    @Test
    fun offline() = render("offline", ChainHealthIndicator.Offline)

    @Test
    fun mixed() = render(
        "mixed",
        item(IndicatorRow.People, share = 0.8f, blockTime = 2.seconds),
        item(IndicatorRow.AssetHub, share = 0.2f, blockTime = 2.seconds),
        item(IndicatorRow.Bulletin, ChainHealthIndicator.Offline),
        item(IndicatorRow.StatementStore, ChainHealthIndicator.Connecting),
    )

    private fun render(name: String, share: Float) = render(
        name,
        item(IndicatorRow.People, share, blockTime = 2.seconds),
        item(IndicatorRow.AssetHub, share, blockTime = 2.seconds),
        item(IndicatorRow.Bulletin, share, blockTime = 6.seconds),
        item(IndicatorRow.StatementStore, CONNECTED),
    )

    private fun render(name: String, all: ChainHealthIndicator) = render(
        name,
        item(IndicatorRow.People, all),
        item(IndicatorRow.AssetHub, all),
        item(IndicatorRow.Bulletin, all),
        item(IndicatorRow.StatementStore, all),
    )

    private fun render(name: String, vararg rows: ChainHealthItemModel) {
        compose.setContent {
            CompositionLocalProvider(LocalInspectionMode provides true) {
                PolkadotTheme {
                    Box(
                        modifier = Modifier
                            .testTag(TAG)
                            .background(PolkadotTheme.colors.bg.surface.main)
                            .padding(16.dp),
                    ) {
                        PolkadotSurface(
                            modifier = Modifier.width(PANEL_WIDTH),
                            shape = RoundedCornerShape(CONTAINER_RADIUS),
                            color = PolkadotTheme.colors.bg.surface.container,
                            border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary),
                        ) {
                            ChainHealthPanel(model = ChainHealthIndicatorsModel(rows.toList().toImmutableList()))
                        }
                    }
                }
            }
        }

        val image = compose.onNodeWithTag(TAG).captureToImage()
        val file = save(image, name)
        assertTrue("$name: no screenshot written", file.length() > 0)
    }

    private fun item(row: IndicatorRow, share: Float, blockTime: Duration) =
        item(row, ChainHealthIndicator.of(share, blockTime))

    private fun item(row: IndicatorRow, indicator: ChainHealthIndicator) = ChainHealthItemModel(
        row = row,
        indicator = indicator,
    )

    // AGP hands the output dir over as a runner argument when it collects test outputs; without it the
    // files land in the test app's external files dir, where `adb pull` can reach them.
    private fun save(image: ImageBitmap, name: String): File {
        val fromRunner = InstrumentationRegistry.getArguments().getString(OUTPUT_DIR_ARGUMENT)?.let(::File)
        val dir = fromRunner ?: File(InstrumentationRegistry.getInstrumentation().context.getExternalFilesDir(null), "panel")
        dir.mkdirs()
        return File(dir, "panel-$name.png").also { file ->
            file.outputStream().use { image.asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }

    private companion object {
        const val TAG = "panel"
        const val OUTPUT_DIR_ARGUMENT = "additionalTestOutputDir"
        val PANEL_WIDTH = 370.dp
        val CONTAINER_RADIUS = 32.dp
        val CONNECTED = ChainHealthIndicator.Healthy(liveness = null)
    }
}
