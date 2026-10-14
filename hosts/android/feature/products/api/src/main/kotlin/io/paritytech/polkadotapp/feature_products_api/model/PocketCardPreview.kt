package io.paritytech.polkadotapp.feature_products_api.model

/**
 * Where a card's static face is read from.
 *
 * A published manifest can only ever name [Archive]: letting it name a [Url] would hand any product
 * on chain a way to make the host fetch an address of its choosing before the user has approved
 * anything. [Url] exists for a worker supplied through the debug menu, which is not published.
 */
sealed interface PocketCardPreview {
    /** A path inside the product's worker archive. */
    data class Archive(val path: String) : PocketCardPreview

    /** An absolute URL, reachable only for a debug-supplied worker. */
    data class Url(val url: String) : PocketCardPreview
}
