package io.paritytech.polkadotapp.database.model

import androidx.room.Entity

/**
 * A Pocket card the user added. No foreign key to `products`: a card's product need not be installed
 * for the card to stay in the collection.
 *
 * The face is held separately in [PocketCardFaceLocal]: it is redrawn as often as the product likes,
 * and membership must not change every time it does. Host-placed cards never land here.
 */
@Entity(
    tableName = "pocket_cards",
    primaryKeys = ["productId", "cardId"],
)
class PocketCardLocal(
    val productId: String,
    val cardId: String,
    val title: String,
)
