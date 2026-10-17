package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import android.app.Activity
import android.app.AlertDialog
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import androidx.core.app.NotificationManagerCompat
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeout
import java.lang.ref.WeakReference
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/** Process-owned leases, never product addressable. No track or picker token crosses the JS boundary. */
internal object MediaServiceLeases {
    data class Lease(val product: String, val types: Int, val stop: () -> Unit, val stopScreen: () -> Unit)
    val leases = ConcurrentHashMap<String, Lease>()
    private val ready = ConcurrentHashMap<String, CompletableDeferred<Unit>>()

    suspend fun acquire(context: Context, id: String, lease: Lease) {
        val previous = leases.put(id, lease)
        val completion = CompletableDeferred<Unit>()
        ready[id] = completion
        try {
            context.startForegroundService(Intent(context, NativeMediaService::class.java))
            withTimeout(10_000) { completion.await() }
        } catch (error: Throwable) {
            if (previous == null) leases.remove(id, lease) else leases.replace(id, lease, previous)
            throw error
        } finally {
            ready.remove(id, completion)
        }
    }

    fun release(context: Context, id: String) {
        if (leases.remove(id) == null) return
        if (leases.isEmpty()) context.stopService(Intent(context, NativeMediaService::class.java))
        else context.startService(Intent(context, NativeMediaService::class.java))
    }

    fun update(context: Context, id: String, types: Int) {
        val lease = leases[id] ?: return
        leases[id] = lease.copy(types = types)
        context.startService(Intent(context, NativeMediaService::class.java))
    }

    fun started() { ready.values.forEach { it.complete(Unit) } }
    fun failed(error: Throwable) { ready.values.forEach { it.completeExceptionally(error) } }
    fun stopped(ids: Set<String>) {
        ids.forEach { ready[it]?.completeExceptionally(IllegalStateException("Media service unavailable")) }
        val current = ids.mapNotNull { leases.remove(it) }
        current.forEach { it.stop() }
    }
}

/** A receive-only call keeps this notification just as a sending/background call does. */
class NativeMediaService : Service() {
    private val adopted = mutableSetOf<String>()
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            "end" -> MediaServiceLeases.leases.values.toList().forEach { it.stop() }
            "screen" -> MediaServiceLeases.leases.values.toList().forEach { it.stopScreen() }
        }
        adopted.addAll(MediaServiceLeases.leases.keys)
        val leases = MediaServiceLeases.leases.values.toList()
        if (leases.isEmpty()) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Product calls", NotificationManager.IMPORTANCE_LOW))
        fun action(name: String) = PendingIntent.getService(this, name.hashCode(),
            Intent(this, NativeMediaService::class.java).setAction(name), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val notification = Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setContentTitle("Product call active")
            .setContentText(leases.map { it.product }.distinct().joinToString())
            .setOngoing(true)
            .setDeleteIntent(action("end"))
            .setCategory(Notification.CATEGORY_CALL)
            .addAction(Notification.Action.Builder(null, "End calls", action("end")).build())
            .addAction(Notification.Action.Builder(null, "Stop sharing screen", action("screen")).build())
            .build()
        val types = leases.fold(ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK) { value, lease -> value or lease.types }
        try {
            startForeground(47923, notification, types)
            MediaServiceLeases.started()
        } catch (failure: SecurityException) {
            MediaServiceLeases.failed(failure)
            MediaServiceLeases.stopped(adopted)
            stopSelf()
        } catch (failure: IllegalStateException) {
            MediaServiceLeases.failed(failure)
            MediaServiceLeases.stopped(adopted)
            stopSelf()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        MediaServiceLeases.stopped(adopted)
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "native-product-media"
        fun indicatorAvailable(context: Context): Boolean {
            if (!NotificationManagerCompat.from(context).areNotificationsEnabled()) return false
            val channel = context.getSystemService(NotificationManager::class.java).getNotificationChannel(CHANNEL)
            return channel == null || channel.importance != NotificationManager.IMPORTANCE_NONE
        }
    }
}

/** Android owns both the projection picker and exact immutable Calling review. */
object NativeMediaConsent {
    internal data class Request(val title: String?, val message: String?, val permission: String?, val result: CompletableDeferred<Intent?>, var activity: WeakReference<Activity>? = null)
    internal val requests = ConcurrentHashMap<String, Request>()

    suspend fun calling(context: Context, product: String, network: ByteArray, account: ByteArray): Boolean {
        fun ByteArray.hex() = joinToString("") { "%02x".format(it) }
        return request(context, "Allow calling?", "Product: $product\nNetwork: ${network.hex()}\nAccount (sr25519): ${account.hex()}\n\nAllow this product to make and receive calls for this account on this network?")
            ?.getBooleanExtra("granted", false) ?: throw CancellationException()
    }

    internal suspend fun screen(context: Context): Intent? = request(context, null, null)

    suspend fun device(context: Context, product: String, permission: String): Boolean {
        val device = if (permission == android.Manifest.permission.CAMERA) "camera" else "microphone"
        return request(context, "Allow $device access?", "Product: $product\n\nAllow this product to use your $device for calling?")
            ?.getBooleanExtra("granted", false) ?: throw CancellationException()
    }

    internal suspend fun operatingSystemPermission(context: Context, permission: String) {
        if (context.checkSelfPermission(permission) == android.content.pm.PackageManager.PERMISSION_GRANTED) return
        val granted = request(context, null, null, permission)?.getBooleanExtra("granted", false)
            ?: throw CancellationException()
        if (!granted) throw SecurityException()
    }

    private suspend fun request(context: Context, title: String?, message: String?, permission: String? = null): Intent? = withContext(Dispatchers.Main.immediate) {
        val id = UUID.randomUUID().toString()
        val request = Request(title, message, permission, CompletableDeferred())
        requests[id] = request
        try {
            context.startActivity(Intent(context, NativeMediaConsentActivity::class.java)
                .putExtra("request", id).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            request.result.await()
        } finally {
            requests.remove(id)
            request.activity?.get()?.finish()
        }
    }
}

class NativeMediaConsentActivity : Activity() {
    private var requestId: String? = null
    private var request: NativeMediaConsent.Request? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestId = intent.getStringExtra("request")
        request = requestId?.let(NativeMediaConsent.requests::get)
        val current = request ?: return finish()
        current.activity = WeakReference(this)
        if (current.permission != null) {
            if (checkSelfPermission(current.permission) == android.content.pm.PackageManager.PERMISSION_GRANTED) {
                current.result.complete(Intent().putExtra("granted", true))
                finish()
            } else if (savedInstanceState == null) requestPermissions(arrayOf(current.permission), 2)
        } else if (current.title == null) {
            if (savedInstanceState == null) {
                startActivityForResult(getSystemService(MediaProjectionManager::class.java).createScreenCaptureIntent(), 1)
            }
        } else {
            AlertDialog.Builder(this).setTitle(current.title).setMessage(current.message)
                .setPositiveButton("Allow") { _, _ ->
                    current.result.complete(Intent().putExtra("granted", true))
                    finish()
                }
                .setNegativeButton("Deny") { _, _ -> current.result.complete(Intent().putExtra("granted", false)); finish() }
                .setOnCancelListener { current.result.completeExceptionally(CancellationException()); finish() }.show()
        }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == 1) {
            request?.result?.complete(data.takeIf { resultCode == RESULT_OK })
            finish()
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 2) {
            if (grantResults.isEmpty()) request?.result?.completeExceptionally(CancellationException())
            else request?.result?.complete(Intent().putExtra("granted", grantResults.all { it == android.content.pm.PackageManager.PERMISSION_GRANTED }))
            finish()
        }
    }

    override fun onDestroy() {
        if (!isChangingConfigurations) request?.result?.completeExceptionally(CancellationException())
        super.onDestroy()
    }
}
