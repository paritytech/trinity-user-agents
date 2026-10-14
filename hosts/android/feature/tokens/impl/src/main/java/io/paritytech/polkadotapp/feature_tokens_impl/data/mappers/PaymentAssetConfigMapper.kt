package io.paritytech.polkadotapp.feature_tokens_impl.data.mappers

import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.feature_tokens_impl.data.paymentAsset.PaymentAssetConfigRemote
import io.paritytech.polkadotapp.feature_tokens_impl.domain.paymentAsset.PaymentAssetConfig

internal fun PaymentAssetConfigRemote.toDomain(): PaymentAssetConfig = PaymentAssetConfig(
    symbol = symbol?.trim()?.takeIf(String::isNotEmpty),
    squareLogoUrl = iconSquareUrl?.trim()?.takeIf(Urls::isAbsoluteWebUrl),
    wideLogoUrl = iconWideUrl?.trim()?.takeIf(Urls::isAbsoluteWebUrl),
)
