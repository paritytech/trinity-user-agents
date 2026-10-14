package io.paritytech.polkadotapp.feature_backup_api.mnemonic.model.parcel

import android.os.Parcelable
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import kotlinx.parcelize.Parcelize

@Parcelize
class MnemonicParcelable(
    val words: String,
    val wordsList: List<String>,
    val entropy: ByteArray
) : Parcelable

fun Mnemonic.toParcelable() = MnemonicParcelable(words, wordList, entropy)

fun MnemonicParcelable.fromParcelable() = Mnemonic(words, wordsList, entropy)
