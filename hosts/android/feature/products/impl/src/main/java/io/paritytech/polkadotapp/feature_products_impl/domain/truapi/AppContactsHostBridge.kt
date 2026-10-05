package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ContactsHostBridge
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDirectory
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.runBlocking
import uniffi.truapi.HostContactLookup
import uniffi.truapi.HostContactMatches
import uniffi.truapi.HostContactPick
import javax.inject.Inject

/**
 * Resolves the contact handles a transaction names back to accounts, and draws
 * the picker. The list never leaves the host: the core asks about handles, and
 * a product gets 32 bytes for the one person the user picked.
 *
 * The lookup runs inline on the core's dispatcher thread, so it answers from
 * the latest contact snapshot and a handle index kept per handle key: a map
 * read, not a store query. Only before the first snapshot does it query.
 */
class AppContactsHostBridge @Inject constructor(
    private val directory: ContactDirectory,
    private val pickLauncher: TrUAPIContactPickLauncher,
) : ContactsHostBridge {
    private class HandleIndex(val handleKey: ByteArray, val accountsByHandle: Map<DataByteArray, ByteArray>)

    private val lock = Any()

    /** Unblocked contacts' accounts, from the latest snapshot. */
    private var accounts: List<ByteArray>? = null

    /** Rebuilt when the contacts or the key change; the key changes with the session. */
    private var index: HandleIndex? = null

    /** Take the latest contacts, dropping the handle index built from the old ones. */
    internal fun update(contacts: List<Contact>) = synchronized(lock) {
        accounts = contacts.map { it.accountId.value }
        index = null
    }

    /**
     * A contact's handle is BLAKE2b-256 keyed with the lookup's handle key over
     * its account, so each current contact is hashed once and every requested
     * handle answered from that. The core re-checks each account returned.
     */
    override fun contacts(lookup: HostContactLookup): HostContactMatches {
        val accountsByHandle = handleIndex(lookup.handleKey)
        return HostContactMatches(accounts = lookup.handles.map { accountsByHandle[it.toDataByteArray()] })
    }

    private fun handleIndex(handleKey: ByteArray): Map<DataByteArray, ByteArray> {
        val snapshot = synchronized(lock) {
            index?.takeIf { it.handleKey.contentEquals(handleKey) }?.let { return it.accountsByHandle }
            accounts
        }
        val current = snapshot ?: runBlocking { directory.getContacts().map { it.accountId.value } }
        val built = current.associateBy { contactHandle(handleKey, it).toDataByteArray() }
        synchronized(lock) {
            // Kept only if no newer snapshot landed while hashing.
            if (accounts === snapshot) index = HandleIndex(handleKey, built)
        }
        return built
    }

    /**
     * Opens the picker and answers with the person the user named. A contact
     * list with nobody in it answers `NoContacts` without a sheet, because a
     * picker offering no one is a dead end the user has to back out of.
     *
     * Resolution does not go through here: a handle minted earlier still
     * resolves, because that goes through [contacts] alone.
     */
    override suspend fun pickContact(productId: String): HostContactPick {
        val options = directory.getContacts().map { contact ->
            ContactPickOption(
                account = contact.accountId.value,
                displayName = contact.username,
            )
        }

        if (options.isEmpty()) return HostContactPick.NoContacts

        val picked = pickLauncher.awaitPick(productId, options)
            ?: return HostContactPick.Dismissed

        return HostContactPick.Picked(picked.account)
    }

    /**
     * Emits whenever someone leaves the user's contacts, removed or blocked.
     * The host passes it on to the core, which drops the contact handles it
     * cached so a removed contact stops resolving. Collecting it also keeps
     * this bridge's lookup snapshot current.
     */
    fun contactRemovals(): Flow<Unit> = directory.observeContacts()
        .onEach(::update)
        .contactRemovals()
}

/** The handle the core knows [account] by under [handleKey]. */
internal fun contactHandle(handleKey: ByteArray, account: ByteArray): ByteArray =
    account.blake2b256(key = handleKey)

/**
 * Emits each time the set of accounts shrinks. The first list is a baseline,
 * and additions are ignored: a handle the core cached names someone who was a
 * contact when it was cached, so a newcomer cannot make it wrong.
 */
internal fun Flow<List<Contact>>.contactRemovals(): Flow<Unit> = flow {
    var known: Set<AccountId>? = null
    collect { contacts ->
        val current = contacts.mapTo(HashSet()) { it.accountId }
        val previous = known
        known = current
        if (previous != null && !current.containsAll(previous)) emit(Unit)
    }
}
