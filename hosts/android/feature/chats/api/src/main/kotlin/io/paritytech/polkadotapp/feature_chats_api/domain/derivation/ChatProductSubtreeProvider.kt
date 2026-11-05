package io.paritytech.polkadotapp.feature_chats_api.domain.derivation

/**
 * Root of the chat product's key subtree, `//product//chat.<tld>` (chat-spec RFC-0005). Contributed by the module
 * that owns product derivation, so chats never has to know how product paths are built.
 */
interface ChatProductSubtreeProvider {
    /** Suspends until the network's TLD is settled: a path built from a guessed TLD would mint keys of no network. */
    suspend fun subtreePath(): String
}
