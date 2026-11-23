package io.paritytech.polkadotapp.feature_usernames_impl.data.signing

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.runtime.metadata.moduleOrNull
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.constantOrNull
import io.paritytech.polkadotapp.chains.util.decoded
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.chains.util.signing.PrefixedSigningContext
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.AccountSigningContexts
import javax.inject.Inject

interface AccountSigningContextRepository {
    suspend fun getContexts(): Result<AccountSigningContexts>
}

class RealAccountSigningContextRepository @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains
) : AccountSigningContextRepository {
    override suspend fun getContexts(): Result<AccountSigningContexts> = runCancellableCatching {
        AccountSigningContexts(
            peopleLite = signingContext(knownChains.people, Modules.PEOPLE_LITE),
            dotNsGateway = signingContext(knownChains.assetHub, Modules.DOTNS_GATEWAY)
        )
    }

    private suspend fun signingContext(chainId: ChainId, palletName: String): MessageSigningContext {
        val runtime = chainRegistry.getRuntime(chainId)
        val prefix = runtime.metadata.moduleOrNull(palletName)
            ?.constantOrNull("SigningContext")
            ?.let { Scale.decode<DataByteArray>(it.decoded(runtime)) }

        return prefix?.let { PrefixedSigningContext(it.value) } ?: MessageSigningContext.trustedContent()
    }
}
