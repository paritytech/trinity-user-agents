package io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue.compose.ReportIssueScreen
import io.paritytech.polkadotapp.common.presentation.notification.AppNotificationHost
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet

@AndroidEntryPoint
class ReportIssueBottomSheet : BaseComposeBottomSheet<ReportIssueViewModel>() {
    override val viewModel: ReportIssueViewModel by viewModels()

    @Composable
    override fun Screen() {
        Box {
            ReportIssueScreen(viewModel)
            Box(Modifier.matchParentSize()) {
                AppNotificationHost(appNotifier)
            }
        }
    }
}
