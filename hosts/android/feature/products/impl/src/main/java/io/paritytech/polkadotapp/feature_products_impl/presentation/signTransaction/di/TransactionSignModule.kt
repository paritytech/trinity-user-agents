package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContext
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.CreateTransactionInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.IdentityAccountSigningSource
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.LegacyAccountSigningSource
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.ProductAccountSigningSource
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.ProductSigningSource
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.SignPayloadJsonInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.SignRawInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.SignVrfInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.TransactionSignInteractor

@Module
@InstallIn(ViewModelComponent::class)
class TransactionSignModule {
    @Provides
    fun provideSigningContext(
        signingContextHolder: SigningContextHolder,
    ): SigningContext {
        return requireNotNull(signingContextHolder.get()) {
            "SigningContext is not set. The signing screen was likely restored after process death."
        }
    }

    @Provides
    fun provideTransactionSignInteractor(
        signingContext: SigningContext,
        signPayloadJsonInteractorFactory: SignPayloadJsonInteractor.Factory,
        signRawInteractorFactory: SignRawInteractor.Factory,
        signVrfInteractorFactory: SignVrfInteractor.Factory,
        createTransactionInteractorFactory: CreateTransactionInteractor.Factory,
        productSigningSourceFactory: ProductAccountSigningSource.Factory,
        identityAccountSigningSource: IdentityAccountSigningSource,
    ): TransactionSignInteractor {
        // The account was resolved before the screen was shown; map it to the signing source.
        val signingSource: ProductSigningSource = when (val account = signingContext.signingAccount) {
            is SigningAccount.Product -> productSigningSourceFactory.create(account.accountId)
            SigningAccount.IdentityAccount -> identityAccountSigningSource
            // Only reachable from the TrUAPI core, which signs these itself, so
            // the source exists to name the account and refuses to sign.
            is SigningAccount.Legacy -> LegacyAccountSigningSource(account.accountId)
        }

        return when (val body = signingContext.signingRequestBody) {
            is SigningRequestBody.Transaction -> signPayloadJsonInteractorFactory.create(body.payload, signingSource)
            is SigningRequestBody.Raw -> signRawInteractorFactory.create(body.payload.type, signingSource)
            is SigningRequestBody.SignVrf -> signVrfInteractorFactory.create(body, signingSource)
            is SigningRequestBody.CreateTransaction -> createTransactionInteractorFactory.create(body.payload, signingSource)
            is SigningRequestBody.RawLegacy -> signRawInteractorFactory.create(body.payload.type, signingSource)
            is SigningRequestBody.TransactionLegacy -> signPayloadJsonInteractorFactory.create(body.payload, signingSource)
            is SigningRequestBody.CreateTransactionLegacy -> createTransactionInteractorFactory.create(body.payload, signingSource)
        }
    }
}
