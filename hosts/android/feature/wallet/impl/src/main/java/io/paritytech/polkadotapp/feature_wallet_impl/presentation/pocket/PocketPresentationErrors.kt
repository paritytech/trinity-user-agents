package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.R as RCommon

class GetCashUnavailablePresentationError(cause: Throwable) : PresentationThrowable(cause) {
    @Composable
    override fun message(): String {
        return stringResource(RCommon.string.pocket_error_get_cash_unavailable, LocalPaymentAssetBrand.current.symbol)
    }
}

class WithdrawUnavailablePresentationError(cause: Throwable) : PresentationThrowable(cause) {
    @Composable
    override fun message(): String {
        return stringResource(RCommon.string.pocket_error_withdraw_unavailable, LocalPaymentAssetBrand.current.symbol)
    }
}

class AutoFundFailedPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.pocket_error_auto_fund_failed)

class ShareCoinageLogsFailedPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.pocket_error_share_coinage_logs_failed)
