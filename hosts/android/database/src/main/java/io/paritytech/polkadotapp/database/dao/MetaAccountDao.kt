package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.MetaAccountLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class MetaAccountDao {
    @Insert
    abstract suspend fun insertMetaAccount(metaAccount: MetaAccountLocal): Long

    @Query("DELETE FROM meta_accounts WHERE id = :metaId")
    abstract suspend fun delete(metaId: Long)

    @Query("SELECT * FROM meta_accounts WHERE id = :metaId")
    abstract suspend fun getMetaAccount(metaId: Long): MetaAccountLocal?

    @Query("SELECT * FROM meta_accounts WHERE purpose = :purpose")
    abstract suspend fun getAccountByPurpose(purpose: MetaAccountLocal.PurposeLocal): MetaAccountLocal

    @Query("SELECT * FROM meta_accounts WHERE purpose = :purpose")
    abstract fun subscribeAccountByPurpose(purpose: MetaAccountLocal.PurposeLocal): Flow<MetaAccountLocal?>

    @Query("SELECT * FROM meta_accounts WHERE purpose = 'WALLET'")
    abstract fun walletAccountFlow(): Flow<MetaAccountLocal?>

    @Query("SELECT EXISTS (SELECT * FROM meta_accounts)")
    abstract suspend fun isAnyMetaAccountExists(): Boolean

    @Query("SELECT EXISTS (SELECT * FROM meta_accounts)")
    abstract fun isAnyMetaAccountExistsFlow(): Flow<Boolean>

    @Query("SELECT * FROM meta_accounts WHERE purpose = 'ALIAS' AND aliasContext = :context")
    abstract suspend fun getAliasAccount(context: ByteArray): MetaAccountLocal?

    @Transaction
    open suspend fun withTransaction(action: suspend () -> Unit) {
        return action()
    }
}
