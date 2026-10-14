package io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.selection.selectable
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun RowScope.ModeTouchTarget(
    appearance: ModeAppearance,
    isSelected: Boolean,
    interactionSource: MutableInteractionSource,
    onClick: () -> Unit
) {
    val itemDescription = appearance.accessibilityDescription
    val itemState = stringResource(
        if (isSelected) {
            RCommon.string.payment_privacy_mode_state_selected
        } else {
            RCommon.string.payment_privacy_mode_state_not_selected
        }
    )

    Box(
        modifier = Modifier
            .weight(1f)
            .fillMaxHeight()
            // A ripple across the whole cell would read as a card press, so the indication is handed to the
            // circle instead — the thing the user is actually choosing.
            .selectable(
                selected = isSelected,
                interactionSource = interactionSource,
                indication = null,
                role = Role.RadioButton,
                onClick = onClick
            )
            .semantics {
                contentDescription = itemDescription
                stateDescription = itemState
            }
    )
}
