package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import uniffi.truapi.BundledStack

/**
 * The Trinity stacks the app bundles, and which one each environment runs.
 *
 * Each stack ships as two assets: `<asset>.link`, the stack's pointer, and `<asset>.json`, its signed
 * descriptor, which `scripts/update-stacks.sh` refreshes from the CLI's copy. The core checks the descriptor
 * against the pointer's key when the runtime starts.
 */
enum class TrUAPIStack(
    private val asset: String,
    // The core refuses Polkadot's genesis hashes and `dot` TLD from any stack without this.
    private val allowsReservedValues: Boolean,
) {
    PREVIEWNET(asset = "stacks/previewnet", allowsReservedValues = false),
    PASEO_NEXT_V2(asset = "stacks/paseo-next-v2", allowsReservedValues = false),
    POLKADOT(asset = "stacks/polkadot", allowsReservedValues = true);

    fun bundled(context: Context): BundledStack {
        fun read(extension: String) = context.assets.open("$asset.$extension").use { it.readBytes() }
        return BundledStack(
            pointer = read("link").decodeToString().trim(),
            descriptor = read("json"),
            allowsReservedValues = allowsReservedValues,
        )
    }

    companion object {
        fun of(environment: TestnetEnvironment): TrUAPIStack = when (environment) {
            TestnetEnvironment.TESTNET -> PREVIEWNET
            TestnetEnvironment.NIGHTLY -> PASEO_NEXT_V2
            TestnetEnvironment.PRODUCTION -> POLKADOT
        }
    }
}
