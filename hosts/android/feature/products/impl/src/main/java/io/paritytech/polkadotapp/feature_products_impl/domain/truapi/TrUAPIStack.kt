package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import uniffi.truapi.BundledStack

/**
 * The Trinity stacks the app ships, which the core takes every chain it uses from. Each [asset] is
 * the signed descriptor as served, which `scripts/update-stacks.sh` refreshes with the CLI's copy,
 * and only verifies under the key in its own [pointer].
 */
enum class TrUAPIStack(
    private val pointer: String,
    private val asset: String,
    // The core refuses Polkadot's genesis hashes and `dot` TLD from any stack without this.
    private val allowsReservedValues: Boolean,
) {
    PREVIEWNET(
        pointer = "polkadotapp://stack?key=0x4d43362f03d5511632a210632d1fb7760b5a663580f12ffca34619f79357e239" +
            "&url=https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json",
        asset = "stacks/previewnet.json",
        allowsReservedValues = false,
    ),
    PASEO_NEXT_V2(
        pointer = "polkadotapp://stack?key=0x2b89d8ca74aa2e11ab55f5af1168cd6929b88d1998aecd979b42cfc1078d5377" +
            "&url=https%3A%2F%2Fstack-paseo-next.parity-testnet.parity.io%2F.well-known%2Ftrinity-stack.json",
        asset = "stacks/paseo-next-v2.json",
        allowsReservedValues = false,
    ),
    POLKADOT(
        pointer = "polkadotapp://stack?key=0xdcddb708759dade2e3850368ce0add2072f424ba53f1a2128f3339d646cffe28" +
            "&url=https%3A%2F%2Fstack.api.pcf.network%2F.well-known%2Ftrinity-stack.json",
        asset = "stacks/polkadot.json",
        allowsReservedValues = true,
    );

    fun bundled(context: Context): BundledStack = BundledStack(
        pointer = pointer,
        descriptor = context.assets.open(asset).use { it.readBytes() },
        allowsReservedValues = allowsReservedValues,
    )

    companion object {
        fun of(environment: TestnetEnvironment): TrUAPIStack = when (environment) {
            TestnetEnvironment.TESTNET -> PREVIEWNET
            TestnetEnvironment.NIGHTLY -> PASEO_NEXT_V2
            TestnetEnvironment.PRODUCTION -> POLKADOT
        }
    }
}
