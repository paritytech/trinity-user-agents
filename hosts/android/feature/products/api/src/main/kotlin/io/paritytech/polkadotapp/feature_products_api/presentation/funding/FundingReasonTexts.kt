package io.paritytech.polkadotapp.feature_products_api.presentation.funding

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingReasonText
import io.paritytech.polkadotapp.feature_products_api.domain.funding.reasonText
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun FundingReasonText.text(): String = when (this) {
    is FundingReasonText.Res -> stringResource(res)
    is FundingReasonText.Verbatim -> text
}

/** "Failed" and the reason, as a line under a session. */
@Composable
fun FundingFailureReason.failedText(): String = stringResource(RCommon.string.funding_failure_failed, reasonText.text())
