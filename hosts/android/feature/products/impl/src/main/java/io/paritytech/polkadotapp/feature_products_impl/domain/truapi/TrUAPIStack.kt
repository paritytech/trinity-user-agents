package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import timber.log.Timber
import uniffi.truapi.BundledStack
import uniffi.truapi.Stack

/**
 * The Trinity stacks the app bundles, and which one each environment runs.
 *
 * Each stack ships as two assets: `<asset>.link`, the stack's pointer, and `<asset>.json`, its signed
 * descriptor. `:bindings:truapi-host` syncs both from the core checkout's `stacks/`, which
 * `scripts/update-stacks.sh` refreshes. The core checks the descriptor against the pointer's key when the
 * runtime starts.
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

    // Null when the bundled copy is missing or refused; the runtime reports the same failure when it starts.
    fun openedOrNull(context: Context): Stack? = runCatching { Stack.openBundled(bundled(context)) }
        .onFailure { Timber.e(it, "truapi: the bundled %s stack could not be opened", name) }
        .getOrNull()

    companion object {
        fun of(environment: TestnetEnvironment): TrUAPIStack = when (environment) {
            TestnetEnvironment.TESTNET -> PREVIEWNET
            TestnetEnvironment.NIGHTLY -> PASEO_NEXT_V2
            TestnetEnvironment.PRODUCTION -> POLKADOT
        }
    }
}
