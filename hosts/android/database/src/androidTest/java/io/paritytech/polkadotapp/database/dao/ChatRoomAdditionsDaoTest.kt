package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ChatRoomAdditionsDaoTest {
    private val room = byteArrayOf(0x01)
    private val otherRoom = byteArrayOf(0x02)

    private lateinit var database: AppDatabase

    @Before
    fun setUp() {
        database = Room.inMemoryDatabaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java
        ).build()
    }

    @After
    fun tearDown() {
        database.close()
    }

    @Test
    fun theContactKeepsTheDeclarationsOfTheRoom() = runBlocking<Unit> {
        val dao = database.chatRoomDeclarationsDao()
        dao.setFeatures(room, byteArrayOf(2, 0, 0))

        dao.setContact(room, byteArrayOf(7))

        val row = dao.get(room)!!
        assertTrue(row.features!!.contentEquals(byteArrayOf(2, 0, 0)))
        assertTrue(row.contact!!.contentEquals(byteArrayOf(7)))
    }

    @Test
    fun onlyTheRoomsWithAContactAreListedAsContactRooms() = runBlocking<Unit> {
        val dao = database.chatRoomDeclarationsDao()
        dao.setFeatures(room, byteArrayOf(2, 0, 0))
        dao.setContact(otherRoom, byteArrayOf(7))

        assertEquals(listOf(otherRoom.toList()), dao.observeWithContact().first().map { it.chatId.toList() })
    }
}
