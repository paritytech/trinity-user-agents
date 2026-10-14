package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy.Companion.IGNORE
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ProductLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ProductDao {
    @Query("SELECT * FROM products")
    abstract fun observeAll(): Flow<List<ProductLocal>>

    @Query("SELECT * FROM products WHERE id = :id")
    abstract suspend fun getById(id: String): ProductLocal?

    // IGNORE, not REPLACE: registration races on the same product would otherwise cascade-delete
    // the existing row's integrations and scheduled notifications.
    @Insert(onConflict = IGNORE)
    abstract suspend fun insert(product: ProductLocal)

    @Query("SELECT userWorkerUrl FROM products WHERE id = :id")
    abstract suspend fun getUserWorkerUrl(id: String): String?

    @Query("DELETE FROM products WHERE id = :id")
    abstract suspend fun deleteById(id: String)

    /** Writes name + icon, leaving `userWorkerUrl` and the row's FK children intact. */
    @Transaction
    open suspend fun upsertResolved(product: ProductLocal) {
        insertIfAbsent(product)
        updateMetadata(
            id = product.id,
            name = product.name,
            iconCid = product.iconCid,
            iconFormat = product.iconFormat,
        )
    }

    /** Writes name + `userWorkerUrl` (debug menu), leaving a resolved icon intact. */
    @Transaction
    open suspend fun upsertManual(product: ProductLocal) {
        insertIfAbsent(product)
        updateName(id = product.id, name = product.name)
        updateUserWorkerUrl(id = product.id, userWorkerUrl = product.userWorkerUrl)
    }

    // Unlike REPLACE, never clobbers an existing row or cascade-deletes its FK children.
    @Insert(onConflict = IGNORE)
    protected abstract suspend fun insertIfAbsent(product: ProductLocal)

    @Query("UPDATE products SET name = :name, iconCid = :iconCid, iconFormat = :iconFormat WHERE id = :id")
    protected abstract suspend fun updateMetadata(
        id: String,
        name: String,
        iconCid: String?,
        iconFormat: String?,
    )

    @Query("UPDATE products SET name = :name WHERE id = :id")
    protected abstract suspend fun updateName(id: String, name: String)

    @Query("UPDATE products SET userWorkerUrl = :userWorkerUrl WHERE id = :id")
    protected abstract suspend fun updateUserWorkerUrl(id: String, userWorkerUrl: String?)
}
