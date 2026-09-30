package io.paritytech.polkadotapp.database.model

import androidx.room.Entity

/**
 * The newest face tree drawn for a Pocket card, kept so the card wears it offline and at the next
 * cold start instead of reverting to the bundled stub.
 *
 * Held for host-placed cards as much as for added ones, which is why it is not a column on
 * [PocketCardLocal]: a face there would make a pinned card look like one the user added.
 */
@Entity(
    tableName = "pocket_card_faces",
    primaryKeys = ["productId", "cardId"],
)
class PocketCardFaceLocal(
    val productId: String,
    val cardId: String,
    val face: ByteArray,
)
