package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.NovaAddressAvatar
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun EnterAmountRecipient(
    address: String?,
    type: ExtractedAddress.DisplayType?,
    avatarColor: AvatarColorScheme
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.extraLargeIncreased),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center
    ) {
        NovaText(
            text = stringResource(RCommon.string.common_to).lowercase(),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
        )

        HorizontalSpacer { small }

        PolkadotSurface(
            shape = PolkadotTheme.shapes.full,
            color = PolkadotTheme.colors.bg.surface.container,
            border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary),
        ) {
            Row(
                Modifier
                    .padding(vertical = PolkadotTheme.spacings.tiny)
                    .padding(
                        start = PolkadotTheme.spacings.tiny,
                        end = PolkadotTheme.spacings.extraMedium
                    ),
                verticalAlignment = Alignment.CenterVertically
            ) {
                when (type) {
                    ExtractedAddress.DisplayType.USERNAME -> PolkadotAvatar(
                        modifier = Modifier.size(28.dp),
                        model = AvatarUiModel.Name(address.orEmpty(), avatarColor),
                    )

                    ExtractedAddress.DisplayType.ADDRESS -> NovaAddressAvatar(28.dp)
                    null -> Unit
                }

                HorizontalSpacer { tiny }

                NovaText(
                    text = address ?: "",
                    style = PolkadotTheme.typography.body.large,
                    maxLines = 1,
                    overflow = TextOverflow.MiddleEllipsis,
                )
            }
        }
    }
}
