package io.paritytech.polkadotapp.common.data.network

// Should be kept in-tact with common/build.gradle
enum class TestnetEnvironment {
    TESTNET, NIGHTLY, PRODUCTION;

    companion object {
        private val DEFAULT = NIGHTLY

        fun fromNameOrDefault(name: String): TestnetEnvironment {
            return runCatching { valueOf(name) }
                .getOrDefault(DEFAULT)
        }
    }
}
