package io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model

/** [InputAlreadyClaimed] is retryable with a different coin selection; the rest are caller bugs. */
sealed class CoinageRegistrationError(message: String) : Throwable(message) {
    data object EmptyTransaction : CoinageRegistrationError(
        "transaction has neither inputs nor outputs"
    )

    /**
     * An immortal extrinsic has no window, so nothing could ever conclude that it can no longer execute and
     * its inputs would stay locked with no path to a terminal verdict.
     */
    data object NotMortal : CoinageRegistrationError(
        "transaction carries no mortal era, so its inputs could never be released"
    )

    /**
     * Only extrinsics this app built are registrable: they report the era anchor they were signed over.
     * One recovered from an external payload carries the anchor's hash but not its number, which is not
     * enough to place the window.
     */
    data object MissingEraAnchor : CoinageRegistrationError(
        "transaction does not report the block its era was anchored to"
    )

    data class OutputNotFresh(val asset: OwnAsset) : CoinageRegistrationError(
        "output $asset is already minted or is a received key"
    )

    data class InputAlreadyClaimed(val input: CoinageInput) : CoinageRegistrationError(
        "input $input is already claimed by a transaction that has not failed"
    )

    data class InputHandedOff(val input: CoinageInput) : CoinageRegistrationError(
        "input $input carries a handoff mark"
    )

    /** The mirror of [InputHandedOff]: an asset a transaction of ours holds cannot also leave the device. */
    data class HandoffOfClaimedAsset(val asset: OwnAsset) : CoinageRegistrationError(
        "$asset is claimed by a transaction that has not failed, so it cannot be handed off"
    )

    /** An asset leaves the device once: a second handoff would give two peers the same private key. */
    data class HandoffOfHandedOffAsset(val asset: OwnAsset) : CoinageRegistrationError(
        "$asset already carries a handoff mark, so it cannot be handed off again"
    )
}
