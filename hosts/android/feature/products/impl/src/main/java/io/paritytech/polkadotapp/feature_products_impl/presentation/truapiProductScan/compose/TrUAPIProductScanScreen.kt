package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan.compose

import androidx.activity.compose.BackHandler
import androidx.camera.viewfinder.core.ImplementationMode
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import io.paritytech.polkadotapp.common.presentation.camera.compose.QrViewfinder
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan.TrUAPIProductScanViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import io.paritytech.polkadotapp.common.R as RCommon

private const val NOT_FOR_THIS_PRODUCT_SHOWN_MS = 2_000L

/**
 * The host's own scanner screen. Only the hint under the title is the product's words, and
 * codes are reported to the view model rather than in a dialog, so scanning never stops.
 */
@Composable
fun TrUAPIProductScanScreen(viewModel: TrUAPIProductScanViewModel) {
    BackHandler(onBack = viewModel::onCloseClicked)

    var notForThisProduct by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        viewModel.notForThisProduct.collect {
            notForThisProduct = true
            delay(NOT_FOR_THIS_PRODUCT_SHOWN_MS)
            notForThisProduct = false
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        QrViewfinder(
            modifier = Modifier.fillMaxSize(),
            surfaceRequestFlow = viewModel.surfaceRequest,
            invalidCodeEvent = remember { MutableSharedFlow() },
            cameraPermissionDeniedFlow = viewModel.cameraPermissionDenied,
            cameraActive = true,
            bindToCamera = viewModel::bindToCamera,
            onInvalidCodeAlertClosed = {},
            onPermissionAlertClosed = viewModel::onPermissionAlertClosed,
            implementationMode = ImplementationMode.EXTERNAL,
            permissionMissingHint = null,
        )

        Image(
            modifier = Modifier.fillMaxSize(),
            painter = painterResource(RCommon.drawable.img_scanner_frame),
            contentScale = ContentScale.Crop,
            contentDescription = null,
        )

        Column(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .systemBarsPadding()
                .padding(horizontal = PolkadotTheme.spacings.extraLargeIncreased),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        ) {
            NovaText(
                text = stringResource(RCommon.string.truapi_product_scan_title, viewModel.productId),
                color = PolkadotTheme.colors.fg.staticWhite,
                style = PolkadotTheme.typography.title.medium,
                textAlign = TextAlign.Center,
            )
            viewModel.hint?.let { hint ->
                NovaText(
                    text = hint,
                    color = PolkadotTheme.colors.fg.staticWhite,
                    textAlign = TextAlign.Center,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }

        if (notForThisProduct) {
            NovaText(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .systemBarsPadding()
                    .padding(PolkadotTheme.spacings.extraLargeIncreased),
                text = stringResource(RCommon.string.truapi_product_scan_not_for_product, viewModel.productId),
                color = PolkadotTheme.colors.fg.staticWhite,
                textAlign = TextAlign.Center,
            )
        }

        IconButton(
            modifier = Modifier
                .systemBarsPadding()
                .padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.smallIncreased),
            onClick = viewModel::onCloseClicked,
        ) {
            NovaIcon(imageVector = NovaIcons.Close, tint = PolkadotTheme.colors.fg.staticWhite)
        }
    }
}
