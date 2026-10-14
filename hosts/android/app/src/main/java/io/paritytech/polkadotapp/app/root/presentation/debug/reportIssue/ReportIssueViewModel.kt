package io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.app.root.domain.debug.ReportIssueInteractor
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import java.io.File
import javax.inject.Inject

@HiltViewModel
class ReportIssueViewModel @Inject constructor(
    private val savedStateHandle: SavedStateHandle,
    private val interactor: ReportIssueInteractor,
    private val router: RootRouter,
) : BaseViewModel(), ReportIssueContract {
    private val screenshot = File(requireNotNull(savedStateHandle.get<String>(ISSUE_SCREENSHOT_PATH)))
    private val description = savedStateHandle.getStateFlow("description", "")
    private val sending = MutableStateFlow(false)
    private val complete = MutableStateFlow(false)

    override val state = combine(description, sending, complete) { text, isSending, isComplete ->
        ReportIssueState(text, screenshot.path, isSending, isComplete)
    }.stateIn(this, SharingStarted.Eagerly, ReportIssueState(description.value, screenshot.path, false, false))

    override fun onDescriptionChanged(description: String) {
        if (!sending.value) savedStateHandle["description"] = description.take(ISSUE_DESCRIPTION_LIMIT)
    }

    override fun onSendClick() {
        if (sending.value || complete.value || description.value.isBlank()) return
        sending.value = true

        launchUnit {
            try {
                interactor.send(description.value, screenshot)
                    .onSuccess { complete.value = true }
                    .onFailure { showPresentationError(ReportIssueError(it)) }
            } finally {
                sending.value = false
            }
        }
    }

    override fun onCloseClick() = router.back()

    override fun onCleared() {
        screenshot.delete()
        super.onCleared()
    }
}
