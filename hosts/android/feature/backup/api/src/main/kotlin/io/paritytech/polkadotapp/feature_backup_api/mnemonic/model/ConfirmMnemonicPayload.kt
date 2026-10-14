package io.paritytech.polkadotapp.feature_backup_api.mnemonic.model

import android.os.Parcelable
import io.paritytech.polkadotapp.feature_backup_api.mnemonic.model.parcel.MnemonicParcelable
import kotlinx.parcelize.Parcelize

@Parcelize
class ConfirmMnemonicPayload(
    val mnemonic: MnemonicParcelable
) : Parcelable
