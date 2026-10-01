package io.paritytech.polkadotapp.e2e_hooks

import android.content.Context
import android.util.Log
import android.webkit.WebView
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_backup_impl.ManualMnemonicInteractor
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.RecoverUsernameUseCase
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.io.File
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

private const val TAG = "HostPlaygroundE2E"
private const val SEED_FILE_NAME = "e2e-seed"
private val STEP_TIMEOUT = 2.minutes

/**
 * Prepares a nightly build for the host-playground e2e run.
 *
 * Every launch opens the app's WebViews to DevTools so the runner can drive
 * them over CDP. When the runner has left a mnemonic in `files/e2e-seed`, the
 * file is consumed and the account is restored through the same path as manual
 * mnemonic recovery, followed by the on-chain username lookup that marks the
 * account onboarded. The outcome is one logcat line under [TAG]. Seeding runs
 * in the background, so the splash screen of the seeding launch still sees no
 * account; the runner relaunches the app once the line appears.
 */
class HostPlaygroundE2EInitializer @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val manualMnemonicInteractor: ManualMnemonicInteractor,
    private val recoverUsername: RecoverUsernameUseCase,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        WebView.setWebContentsDebuggingEnabled(true)

        val seedFile = File(context.filesDir, SEED_FILE_NAME)
        if (!seedFile.exists()) return@runCatching
        Log.i(TAG, "seeding")

        val mnemonic = seedFile.readText().trim()
        seedFile.delete()

        scope.launch(Dispatchers.IO) {
            when (val failure = seed(mnemonic)) {
                null -> Log.i(TAG, "seeded")
                else -> Log.e(TAG, "seed failed: $failure")
            }
        }
    }

    /** Restores the account and its username, returning why it failed or null on success. */
    private suspend fun seed(mnemonic: String): String? {
        // Both steps wait on the people chain without a deadline of their own: the restore
        // derives keys under the network's dotNS TLD, and the username is read from the chain.
        val restored = withTimeoutOrNull(STEP_TIMEOUT) {
            manualMnemonicInteractor.restoreAccountWithEnteredMnemonic(mnemonic)
        } ?: return "account restore did not finish within $STEP_TIMEOUT"
        // The restore error is reported by type only: its message can quote the words it rejected.
        restored.exceptionOrNull()?.let { return "account restore threw ${it::class.simpleName}" }
        Log.i(TAG, "account restored, recovering the username")

        val recovered = withTimeoutOrNull(STEP_TIMEOUT) { recoverUsername() }
            ?: return "username recovery did not finish within $STEP_TIMEOUT"
        return recovered.fold(
            onSuccess = { found -> if (found) null else "no username is registered for this account" },
            onFailure = { "username recovery threw ${it::class.simpleName}: ${it.message}" },
        )
    }
}
