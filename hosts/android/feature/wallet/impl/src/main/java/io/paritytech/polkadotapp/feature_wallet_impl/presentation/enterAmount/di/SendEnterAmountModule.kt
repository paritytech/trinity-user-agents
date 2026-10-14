package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.hilt.android.scopes.ViewModelScoped
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.feature_coinage_api.domain.submitter.CoinsSubmitter
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.RealSendEnterAmountInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.SendEnterAmountInteractor

@Module
@InstallIn(ViewModelComponent::class)
interface SendEnterAmountModule {
    @Binds
    @ViewModelScoped
    fun bindSendEnterAmountInteractor(impl: RealSendEnterAmountInteractor): SendEnterAmountInteractor

    /** Submitter implementations are contributed by feature modules via `@IntoMap`. */
    @Multibinds
    fun coinsSubmitters(): Map<String, CoinsSubmitter>
}
