package io.paritytech.polkadotapp.feature_backup_impl.recover

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.common.utils.observeWhenStarted
import io.paritytech.polkadotapp.feature_backup_impl.recover.compose.RecoverOptionsScreen

@AndroidEntryPoint
class RecoverOptionsBottomSheet : BaseComposeBottomSheet<RecoverOptionsViewModel>() {
    override val viewModel: RecoverOptionsViewModel by viewModels()

    @Composable
    override fun Screen() {
        RecoverOptionsScreen(viewModel)
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)

        viewModel.isRecovering.observeWhenStarted { isRecovering ->
            isCancelable = !isRecovering
            bottomSheetBehavior?.isDraggable = !isRecovering
        }
    }
}
