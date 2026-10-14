package io.paritytech.polkadotapp.chains.multiNetwork

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.Ids
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment

class KnownChains(
    val people: ChainId,
    val assetHub: ChainId,
    val bulletIn: ChainId,
    val hydration: ChainId?,
) {
    companion object {
        fun createFor(environment: TestnetEnvironment): KnownChains {
            return when (environment) {
                TestnetEnvironment.TESTNET -> KnownChains(
                    people = Chain.Ids.PREVIEWNET_PEOPLE,
                    assetHub = Chain.Ids.PREVIEWNET_ASSET_HUB,
                    bulletIn = Chain.Ids.PREVIEWNET_BULLET_IN,
                    hydration = null
                )

                TestnetEnvironment.NIGHTLY -> KnownChains(
                    people = Chain.Ids.NIGHTLY_PEOPLE,
                    assetHub = Chain.Ids.NIGHTLY_ASSET_HUB,
                    bulletIn = Chain.Ids.NIGHTLY_BULLET_IN,
                    hydration = null
                )

                TestnetEnvironment.PRODUCTION -> KnownChains(
                    people = Chain.Ids.RELEASE_PEOPLE,
                    assetHub = Chain.Ids.RELEASE_ASSET_HUB,
                    bulletIn = Chain.Ids.RELEASE_BULLETIN,
                    hydration = null
                )
            }
        }
    }
}
