package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/**
 * Coins and vouchers start carrying how private they are.
 *
 * Every column is additive and nullable or defaulted, so no existing row needs rewriting. Coins keep a null
 * fungibility, which reads as "origin never observed" — correct for every coin minted before this, since the
 * app had no way to record where it came from. A voucher's current fungibility defaults to zero, which reads
 * as "no anonymity to draw on", and the location service replaces it on its next tick for any voucher
 * actually sitting in a ring. Its frozen maximum stays null, which reads as "never frozen", so the location
 * service freezes a real value for rows that entered their ring before this column existed.
 */
class Migration63To64 : Migration(63, 64) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE coins ADD COLUMN recyclerFungibility INTEGER")
        db.execSQL("ALTER TABLE coins ADD COLUMN hops BLOB")
        db.execSQL("ALTER TABLE coins ADD COLUMN incomingBundleSize INTEGER")

        db.execSQL("ALTER TABLE recycler_vouchers ADD COLUMN recyclerFungibility INTEGER NOT NULL DEFAULT 0")
        db.execSQL("ALTER TABLE recycler_vouchers ADD COLUMN maxRecyclerFungibility INTEGER")
    }
}
