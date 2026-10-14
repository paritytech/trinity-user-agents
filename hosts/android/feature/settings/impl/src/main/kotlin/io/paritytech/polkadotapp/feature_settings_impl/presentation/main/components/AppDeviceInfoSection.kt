package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components

import android.os.Build
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.icons.PolkadotLogo
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun AppDeviceInfoSection(showDeviceDetails: Boolean) {
    val context = LocalContext.current
    val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.large,
                vertical = PolkadotTheme.spacings.mediumIncreased,
            ),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
    ) {
        Image(
            imageVector = PolkadotLogo,
            contentDescription = "polkadot_logo",
            colorFilter = ColorFilter.tint(PolkadotTheme.colors.fg.primary)
        )
        if (showDeviceDetails) {
            InfoText(stringResource(RCommon.string.settings_app_version, packageInfo.versionName.orEmpty(), packageInfo.longVersionCode))

            InfoText(stringResource(RCommon.string.settings_android_version, Build.VERSION.RELEASE, Build.VERSION.SDK_INT))

            InfoText(stringResource(RCommon.string.settings_device_model, "${Build.MANUFACTURER} ${Build.MODEL}"))
        } else {
            InfoText(stringResource(RCommon.string.settings_app_version, packageInfo.versionName.orEmpty(), packageInfo.longVersionCode))
        }
    }
}

@Composable
private fun InfoText(text: String) {
    NovaText(
        text = text,
        color = PolkadotTheme.colors.fg.tertiary,
        style = PolkadotTheme.typography.body.medium
    )
}
