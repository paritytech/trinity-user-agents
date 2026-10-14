package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageTestnetFundUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.OnboardingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.TestnetTransactionOrigins
import java.math.BigDecimal
import javax.inject.Inject

class RealCoinageTestnetFundUseCase @Inject constructor(
    private val testnetTransactionOrigins: TestnetTransactionOrigins,
    private val onboardingUseCase: OnboardingUseCase,
) : CoinageTestnetFundUseCase {
    override suspend fun invoke(amount: BigDecimal): Result<Unit> {
        coinageLogI("testnet-fund onboarding amount=$amount")

        return onboardingUseCase.onboard(amount, testnetTransactionOrigins.fundingOrigin().signerSource)
    }
}
