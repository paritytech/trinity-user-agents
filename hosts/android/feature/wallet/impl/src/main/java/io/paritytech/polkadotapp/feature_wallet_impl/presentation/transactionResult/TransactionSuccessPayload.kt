package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult

import android.os.Parcelable
import kotlinx.parcelize.Parcelize

@Parcelize
class TransactionSuccessPayload(
    val unfinalizedCoins: List<ByteArray>?,
) : Parcelable
