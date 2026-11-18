package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment

/**
 * The chains of each build's own network, as lower-case hex genesis hashes: its relay, Asset Hub,
 * People and Bulletin, matching the stack the build bundles. Every one is in truapi-provider's
 * bundled catalog, which the light client can only reach.
 */
object LightClientChains {
    fun of(environment: TestnetEnvironment): Set<String> = when (environment) {
        TestnetEnvironment.TESTNET -> setOf(
            "860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92",
            "bac97e23fc8f4bccae72a98f8aeb2bcab20bf755862304e4b46ad6473456e896",
            "55e3e689ecfa9d2fffcf7d309b8011956671493982230bfd0420c683542249e9",
            "a081192b90c1f6a3f8e9ce7b2a8246f41af805c66456c84e05fd97c2b3502425",
        )

        TestnetEnvironment.NIGHTLY -> setOf(
            "374057be67b355151f271ff70c3db98308c62c8adc48dc6724b6a009a1a014fd",
            "4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a",
            "4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
            "8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
        )

        // The catalog bundles no Polkadot chains.
        TestnetEnvironment.PRODUCTION -> emptySet()
    }
}
