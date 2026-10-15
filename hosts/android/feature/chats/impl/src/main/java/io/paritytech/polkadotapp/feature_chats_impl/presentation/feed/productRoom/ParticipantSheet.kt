package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import androidx.compose.runtime.Immutable
import io.novasama.substrate_sdk_android.ss58.SS58Encoder
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAddress
import io.paritytech.polkadotapp.chains.util.DEFAULT_PREFIX
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.inMenuOrder
import kotlinx.collections.immutable.ImmutableList

/** An entry of the sheet of one participant of a product room (02-DESIGN §4, scenario S31). */
@Immutable
sealed interface ParticipantSheetEntry {
    /** Copies the address of a lite person. */
    data class CopyAddress(val address: String) : ParticipantSheetEntry

    /** Opens the App of a product at [address]. */
    data class OpenProduct(val address: String) : ParticipantSheetEntry

    /** An entry of the product (`participant_actions_query`). [icon] is what the image loader draws, or null. */
    data class Product(val actionId: String, val label: String, val icon: Any?, val isDestructive: Boolean) : ParticipantSheetEntry
}

/** The sheet of one participant. [loading] is true while the product did not answer the menu of the participant. */
@Immutable
data class ParticipantSheetUiState(
    val participant: ChatParticipantUiModel,
    val entries: ImmutableList<ParticipantSheetEntry>,
    val loading: Boolean,
)

internal object ParticipantSheet {
    /**
     * The host entry for the kind of the participant. The host has no People profile or contact lookup for a handle
     * yet, so a person and a contact have no host entry (as iOS). [roomProductAddress] is the App of the product of the room.
     */
    fun builtIns(participantId: String, entry: ChatParticipantEntryModel?, roomProductAddress: String?): List<ParticipantSheetEntry> {
        if (participantId == ProductParticipant.PRODUCT) return listOfNotNull(roomProductAddress?.let(ParticipantSheetEntry::OpenProduct))

        return when (val participant = entry?.participant) {
            is ChatParticipantModel.LitePerson -> listOf(ParticipantSheetEntry.CopyAddress(participant.account.address()))
            is ChatParticipantModel.Product -> listOf(ParticipantSheetEntry.OpenProduct(Urls.ensureHasProtocolOrHttps(participant.productId)))
            is ChatParticipantModel.Person,
            is ChatParticipantModel.Contact,
            is ChatParticipantModel.Custom,
            null -> emptyList()
        }
    }

    /**
     * `Custom` drops the host entry. `ExtendBuiltIn` keeps it, then adds the product entries. No answer keeps the host
     * entry only. A destructive product entry goes last.
     */
    fun entries(builtIns: List<ParticipantSheetEntry>, menu: ChatParticipantMenuModel?, icons: Map<String, Any>): List<ParticipantSheetEntry> =
        when (menu) {
            null -> builtIns
            is ChatParticipantMenuModel.Custom -> menu.actions.toEntries(icons)
            is ChatParticipantMenuModel.ExtendBuiltIn -> builtIns + menu.actions.toEntries(icons)
        }

    private fun List<ChatProductActionModel>.toEntries(icons: Map<String, Any>) = inMenuOrder().map {
        ParticipantSheetEntry.Product(actionId = it.actionId, label = it.label, icon = icons[it.actionId], isDestructive = it.destructive)
    }

    /** The SS58 address of an account with the generic prefix, or its hex when the account is not 32 bytes. */
    private fun DataByteArray.address(): String =
        runCatching { value.toAddress(SS58Encoder.DEFAULT_PREFIX) }.getOrElse { "0x" + value.joinToString("") { "%02x".format(it) } }
}

internal fun ChatParticipantMenuModel.actions(): List<ChatProductActionModel> = when (this) {
    is ChatParticipantMenuModel.Custom -> actions
    is ChatParticipantMenuModel.ExtendBuiltIn -> actions
}
