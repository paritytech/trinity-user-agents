package io.paritytech.polkadotapp.common.presentation.clipboard

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import javax.inject.Inject
import javax.inject.Singleton

interface ClipboardService {
    fun observePrimaryClip(): Flow<String?>

    fun getPrimaryClip(): String?

    fun setPrimaryClip(content: String)
}

@Singleton
internal class RealClipboardService @Inject constructor(
    @ApplicationContext context: Context
) : ClipboardService {
    private val clipboardManager = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager

    override fun observePrimaryClip(): Flow<String?> = callbackFlow {
        send(getPrimaryClip())

        val listener = ClipboardManager.OnPrimaryClipChangedListener {
            trySend(getPrimaryClip())
        }

        clipboardManager.addPrimaryClipChangedListener(listener)

        awaitClose {
            clipboardManager.removePrimaryClipChangedListener(listener)
        }
    }

    override fun getPrimaryClip(): String? {
        return with(clipboardManager) {
            if (!hasPrimaryClip()) {
                null
            } else {
                val item: ClipData.Item = primaryClip!!.getItemAt(0)

                item.text?.toString()
            }
        }
    }

    override fun setPrimaryClip(content: String) {
        val clip = ClipData.newPlainText(DEFAULT_LABEL, content)
        clipboardManager.setPrimaryClip(clip)
    }
}

private const val DEFAULT_LABEL = "PolkadotApp"
