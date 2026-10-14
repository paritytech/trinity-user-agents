package io.paritytech.polkadotapp.feature_coinage_impl.domain

import timber.log.Timber

const val COINAGE_LOG_TAG = "CoinageTransfer"

fun coinageLogD(message: String) {
    Timber.tag(COINAGE_LOG_TAG).d(message)
}

fun coinageLogI(message: String) {
    Timber.tag(COINAGE_LOG_TAG).i(message)
}

fun coinageLogW(message: String) {
    Timber.tag(COINAGE_LOG_TAG).w(message)
}

fun coinageLogE(message: String, throwable: Throwable? = null) {
    Timber.tag(COINAGE_LOG_TAG).e(throwable, message)
}
