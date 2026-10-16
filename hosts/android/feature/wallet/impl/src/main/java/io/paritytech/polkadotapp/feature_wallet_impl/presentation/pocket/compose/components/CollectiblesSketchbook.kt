package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.feature_wallet_impl.R
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.LocalCardTilt
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.MotionShineParameters
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.motionShine
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CollectiblesSketchbook(
    modifier: Modifier = Modifier,
    onViewButtonClick: () -> Unit,
    blackAndWhite: Boolean = false,
    expanded: Boolean = false
) {
    val tiltState = LocalCardTilt.current
    val shineParameters = remember(expanded) { MotionShineParameters.collectibles(expanded) }

    Box(
        modifier = modifier
    ) {
        Image(
            modifier = Modifier.fillMaxWidth(),
            painter = painterResource(R.drawable.img_pocket_collectibles),
            contentDescription = "img_pocket_collectibles",
            contentScale = ContentScale.FillWidth,
            colorFilter = remember(blackAndWhite) {
                if (blackAndWhite) {
                    val matrix = ColorMatrix().apply {
                        setToSaturation(0f)
                    }

                    ColorFilter.colorMatrix(matrix)
                } else {
                    null
                }
            }
        )

        Box(
            modifier = Modifier
                .matchParentSize()
                .motionShine(tiltState, shineParameters)
        )

        PolkadotTextButton(
            modifier = Modifier.align(Alignment.Center),
            text = stringResource(RCommon.string.pocket_collectibles_open_button),
            onClick = onViewButtonClick,
            shape = PolkadotButtonShape.pill
        )
    }
}
