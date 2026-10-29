package io.paritytech.polkadotapp.feature_settings_impl.presentation.common

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.AlertOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.CloudOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.ShieldCheck
import io.paritytech.polkadotapp.design.components.menu.PolkadotMenuListItem
import io.paritytech.polkadotapp.design.components.menu.PolkadotMenuListScope
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PolkadotMenuListScope.SettingsMenuItem(
    onClick: () -> Unit,
    title: String,
    icon: ImageVector? = null,
    label: String? = null
) {
    PolkadotMenuListItem(
        leading = icon?.let {
            {
                NovaIcon(
                    modifier = Modifier
                        .size(20.dp)
                        .align(Alignment.Center),
                    imageVector = it,
                    tint = PolkadotTheme.colors.fg.secondary
                )
            }
        },
        title = {
            NovaText(
                text = title,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        },
        trailing = { MenuValueChevron(label) },
        onClick = onClick
    )
}

@Composable
fun PolkadotMenuListScope.BackupSettingsMenuItem(
    onClick: () -> Unit,
    isBackupMissing: Boolean
) {
    PolkadotMenuListItem(
        leading = {
            NovaIcon(
                modifier = Modifier
                    .size(20.dp)
                    .align(Alignment.Center),
                imageVector = NovaIcons.CloudOutlined,
                tint = PolkadotTheme.colors.fg.secondary
            )
        },
        title = {
            NovaText(
                text = stringResource(RCommon.string.settings_backup),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        },
        description = {
            if (isBackupMissing) {
                NovaText(
                    text = stringResource(RCommon.string.settings_backup_not_backed_up),
                    color = PolkadotTheme.colors.fg.warning
                )
            }
        },
        trailing = {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
            ) {
                if (isBackupMissing) {
                    NovaIcon(
                        modifier = Modifier
                            .size(20.dp),
                        imageVector = NovaIcons.AlertOutlined,
                        tint = PolkadotTheme.colors.fg.warning
                    )
                }

                Chevron()
            }
        },
        onClick = onClick
    )
}

@Composable
fun PolkadotMenuListScope.PaymentsBackupSettingsMenuItem(status: CoinageAccountBackupStatus) {
    PolkadotMenuListItem(
        leading = {
            NovaIcon(
                modifier = Modifier
                    .size(20.dp)
                    .align(Alignment.Center),
                imageVector = NovaIcons.ShieldCheck,
                tint = PolkadotTheme.colors.fg.secondary
            )
        },
        title = {
            NovaText(
                text = stringResource(RCommon.string.settings_payments_backup),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        },
        trailing = { MenuValue(stringResource(status.labelRes())) },
        onClick = null
    )
}

@StringRes
private fun CoinageAccountBackupStatus.labelRes(): Int = when (this) {
    CoinageAccountBackupStatus.Registering -> RCommon.string.settings_payments_backup_in_progress
    CoinageAccountBackupStatus.Delayed -> RCommon.string.settings_payments_backup_delayed
    CoinageAccountBackupStatus.Completed -> RCommon.string.settings_payments_backup_done
}

@Composable
private fun MenuValueChevron(label: String?) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically
    ) {
        label?.let { MenuValue(it) }
        Chevron()
    }
}

@Composable
private fun MenuValue(label: String) {
    NovaText(
        text = label,
        style = PolkadotTheme.typography.body.mediumEmphasized,
        color = PolkadotTheme.colors.fg.secondary
    )
}

@Composable
private fun Chevron() {
    NovaIcon(
        modifier = Modifier
            .size(20.dp),
        imageVector = NovaIcons.ArrowRight,
        tint = PolkadotTheme.colors.fg.secondary
    )
}
