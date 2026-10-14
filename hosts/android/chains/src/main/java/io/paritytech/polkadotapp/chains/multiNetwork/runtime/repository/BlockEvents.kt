package io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent

class BlockEvents(
    val initialization: List<GenericEvent.Instance>,
    val applyExtrinsic: List<ExtrinsicWithEvents>,
    val finalization: List<GenericEvent.Instance>
)
