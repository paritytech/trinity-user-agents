package io.paritytech.polkadotapp.feature_account_api.di

import dagger.MapKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount

@MapKey
@Retention(AnnotationRetention.RUNTIME)
annotation class AccountPurposeKey(val purpose: MetaAccount.Purpose)
