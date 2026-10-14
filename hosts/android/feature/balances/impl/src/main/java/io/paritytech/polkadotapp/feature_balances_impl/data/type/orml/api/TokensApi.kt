package io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.api

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.UntypedOrmlCurrencyId
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry2
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage2
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.OrmlAssetAccount

@JvmInline
internal value class TokensApi(override val module: Module) : QueryableModule

internal val RuntimeMetadata.tokens: TokensApi
    get() = TokensApi(module(Modules.TOKENS))

context(withRuntime: WithRuntime)
internal val TokensApi.accounts: QueryableStorageEntry2<AccountId, UntypedOrmlCurrencyId, OrmlAssetAccount>
    get() = storage2("Accounts")
