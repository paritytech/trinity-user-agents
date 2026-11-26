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
            "48ab0689e87be8c502ace5f1c953baf4337daad39406f1e596c5f2fed5950fe6",
            "f7d88e2e343e911c9a5c1a7e6ed53d06ef4cd73276fc3afff87062a1f6f90514",
            "732abb4e3052154ebd9201a375b0aacf2d1fe7015700235c0a8b30d44b401c19",
            "1c0a1c27946ca0e7def02955315633db590bc38b0c537936e423efeb6c554876",
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
