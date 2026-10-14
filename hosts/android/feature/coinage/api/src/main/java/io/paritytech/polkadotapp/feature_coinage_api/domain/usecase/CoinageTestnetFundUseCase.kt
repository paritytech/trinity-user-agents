package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import java.math.BigDecimal

/**
 * Funds coinage on a testnet by onboarding [amount] against the environment's funding account.
 *
 * Deliberately not a transfer into our own deposit account. A deposit is watched and converted by a service
 * that re-onboards whenever the balance it sees differs from the last one it acted on, so funding that way
 * onboards the remainder a second time and credits more than was asked for. Onboarding takes the external
 * asset straight from the funding account and mints our vouchers, with nothing left behind to be noticed.
 */
interface CoinageTestnetFundUseCase {
    suspend operator fun invoke(amount: BigDecimal): Result<Unit>
}
