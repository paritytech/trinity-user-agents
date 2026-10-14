package io.paritytech.polkadotapp.feature_calls_impl.presentation

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Bundle
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.CallViewModel
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.CallScreen
import io.paritytech.polkadotapp.feature_calls_impl.state.CallStateHolder
import javax.inject.Inject

@AndroidEntryPoint
class CallActivity : AppCompatActivity() {
    @Inject
    lateinit var callStateHolder: CallStateHolder

    private val viewModel: CallViewModel by viewModels()

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { results ->
        if (results.all { it.value }) {
            viewModel.acceptCall()
        } else {
            viewModel.declineCall()
            finish()
        }
    }

    companion object {
        private const val ACTION_ANSWER = "io.paritytech.polkadotapp.action.ANSWER"

        fun newIntent(context: Context): Intent {
            return Intent(context, CallActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }

        fun answerIntent(context: Context): Intent {
            return newIntent(context).setAction(ACTION_ANSWER)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT)
        )

        super.onCreate(savedInstanceState)

        setContent {
            PolkadotTheme {
                CallScreen(
                    viewModel = viewModel,
                    closeCallScreen = { finish() }
                )
            }
        }

        if (intent.action == ACTION_ANSWER) {
            handleAnswerAction()
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.action == ACTION_ANSWER) {
            handleAnswerAction()
        }
    }

    private fun handleAnswerAction() {
        // Use Activity-scoped permission launcher rather than PermissionAsker:
        // PermissionAsker resolves the activity through ContextManager (attached only by RootActivity),
        // so it cannot target CallActivity launched from a notification.
        val withVideo = callStateHolder.getActiveCall()?.initiatedWithVideo == true
        val required = buildList {
            add(Manifest.permission.RECORD_AUDIO)
            if (withVideo) {
                add(Manifest.permission.CAMERA)
            }
        }.toTypedArray()
        val allGranted = required.all { permission ->
            ContextCompat.checkSelfPermission(this, permission) == PackageManager.PERMISSION_GRANTED
        }
        if (allGranted) {
            viewModel.acceptCall()
        } else {
            permissionLauncher.launch(required)
        }
    }
}
