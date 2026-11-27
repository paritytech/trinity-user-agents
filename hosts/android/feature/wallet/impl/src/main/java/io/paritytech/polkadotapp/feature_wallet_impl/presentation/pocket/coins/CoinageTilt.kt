package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.sqrt

/**
 * Turns the studio with the phone, so the light behaves as though it were fixed in the room.
 *
 * The environment the coins are lit against is defined around the coin's own space, so with the studio held
 * still the highlight sits in the same place however the phone is held. Feeding the device's own tilt back
 * in turns the studio the other way, and a coin catches the light as you tilt it, the way a real one does.
 *
 * The turn is the one rotation that carries the neutral gravity reading onto the current one, scaled into the
 * travel and reversed, so the room stays put while the phone moves under it: tilt the screen to face upward and
 * a face-on coin mirrors the ceiling. Gravity cannot see a turn about the vertical, so this is the part of the
 * phone's movement it can see. The coins themselves never move with the phone.
 *
 * `TYPE_GRAVITY` needs no permission and no prompt. It is a fused reading and not present on every device,
 * so the plain accelerometer stands in; at rest the two agree, which is when the studio is allowed to move.
 */
class CoinageTilt(context: Context) : SensorEventListener {
    /**
     * Where the studio is turned to: an axis scaled by the angle turned about it, in radians.
     *
     * One vector rather than a pair of angles, because a rotation about a slanted axis is not the sum of two
     * rotations about the axes either side of it, and the phone rotates about whatever axis the wrist chose.
     */
    data class Turn(val x: Double = 0.0, val y: Double = 0.0, val z: Double = 0.0)

    /**
     * One reading: which way is down, in the phone's own frame.
     *
     * Gravity is all the studio needs and all it can have. It fixes the phone's orientation up to a turn
     * about the vertical, which is exactly the turn a room's lighting does not care about, and unlike
     * integrated attitude it never drifts.
     */
    class Pose private constructor(val downX: Double, val downY: Double, val downZ: Double) {
        /**
         * How far the phone turned between two readings, whatever axis it turned about. Against the cross
         * product rather than from the dot alone: `acos` loses most of its precision for the small angles
         * between consecutive readings, which is where this is asked most.
         */
        fun angleTo(other: Pose): Double {
            val crossX = downY * other.downZ - downZ * other.downY
            val crossY = downZ * other.downX - downX * other.downZ
            val crossZ = downX * other.downY - downY * other.downX
            val dot = downX * other.downX + downY * other.downY + downZ * other.downZ

            return atan2(sqrt(crossX * crossX + crossY * crossY + crossZ * crossZ), dot)
        }

        /** Where the studio sits with the phone here, against this reading as square. */
        fun turnTo(other: Pose): Turn {
            val axisX = downY * other.downZ - downZ * other.downY
            val axisY = downZ * other.downX - downX * other.downZ
            val axisZ = downX * other.downY - downY * other.downX
            val sine = sqrt(axisX * axisX + axisY * axisY + axisZ * axisZ)

            if (sine < 1e-9) return Turn()

            val dot = downX * other.downX + downY * other.downY + downZ * other.downZ
            val turned = -scaled(atan2(sine, dot)) / sine

            return Turn(axisX * turned, axisY * turned, axisZ * turned)
        }

        /**
         * Eases toward another reading, staying a direction rather than shrinking toward the middle: neutral
         * is which way is down, and a blend of two directions has no length of its own to keep.
         */
        fun blended(other: Pose, blend: Double) = of(
            downX + (other.downX - downX) * blend,
            downY + (other.downY - downY) * blend,
            downZ + (other.downZ - downZ) * blend
        )

        companion object {
            fun of(x: Double, y: Double, z: Double): Pose {
                val length = maxOf(sqrt(x * x + y * y + z * z), 1e-9)

                return Pose(x / length, y / length, z / length)
            }

            /**
             * Android reports gravity as proper acceleration — it points *away* from the earth, the opposite
             * of iOS's `CMDeviceMotion.gravity`. The axes are otherwise the same, so negating is the whole
             * of the conversion.
             */
            fun ofSensor(values: FloatArray) =
                of(-values[0].toDouble(), -values[1].toDouble(), -values[2].toDouble())
        }
    }

    private val sensors = context.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
    private val sensor = sensors?.let {
        it.getDefaultSensor(Sensor.TYPE_GRAVITY) ?: it.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
    }

    private var neutral: Pose? = null
    private var previous: Pose? = null
    private var stillFor = 0.0
    private var lastReading = 0L
    private var lastChange = 0L

    @Volatile
    private var readings: HandlerThread? = null

    @Volatile
    private var target = Turn()

    @Volatile
    var onReading: ((sensed: Long, delivered: Long) -> Unit)? = null

    @Volatile
    var turn = Turn()
        private set

    /**
     * Called when the phone has moved enough that the studio needs to catch up. The view pauses itself
     * whenever nothing is moving, so without a push from outside it would never look at the attitude again
     * and the light would stay where the field last settled.
     */
    var onMove: (() -> Unit)? = null

    /**
     * Readings arrive on a thread of their own rather than the main one, so a busy UI frame never leaves
     * them queued and ageing behind it.
     */
    fun start() {
        val sensor = sensor ?: return
        val thread = readings ?: HandlerThread("coinage-tilt").also {
            it.start()
            readings = it
        }

        sensors?.registerListener(this, sensor, INTERVAL_MICROSECONDS, Handler(thread.looper))
    }

    /**
     * Lets go of the sensor. A pause rather than a teardown: [onMove] is the caller's and survives, so the
     * same instance can be started again when the card comes back on screen.
     *
     * Neutral does not survive. A phone that was put somewhere else while nobody was looking should come
     * back square, and the first reading after starting is taken as neutral outright.
     */
    fun stop() {
        sensors?.unregisterListener(this)

        val thread = readings
        readings = null

        // Reset on the readings thread, behind any reading already queued there, so none lands afterwards.
        if (thread != null) {
            Handler(thread.looper).post(::reset)
            thread.quitSafely()
        } else {
            reset()
        }
    }

    private fun reset() {
        neutral = null
        previous = null
        stillFor = 0.0
        lastReading = 0L
    }

    /**
     * Takes the latest reading outright: the fused gravity is steady enough not to shimmer, and the filter
     * that chased it was a longer delay than the rest of the pipeline put together.
     *
     * Returns whether the studio is still moving. It counts as moving for [LINGER] after the last change,
     * since readings come slower than frames and a hand mid-gesture should not put the loop to sleep between
     * two of them.
     */
    fun advance(now: Long): Boolean {
        val next = target

        if (distance(next, turn) > REST_BAND) {
            turn = next
            lastChange = now

            return true
        }

        return now - lastChange < LINGER
    }

    override fun onSensorChanged(event: SensorEvent) {
        // The sampling period is only a hint, so the real gap decides how much of it counts as stillness.
        val elapsed = if (lastReading == 0L) {
            INTERVAL_SECONDS
        } else {
            ((event.timestamp - lastReading) / NANOSECONDS).coerceIn(MIN_INTERVAL, MAX_INTERVAL)
        }
        lastReading = event.timestamp

        absorb(Pose.ofSensor(event.values), elapsed)

        val onReading = onReading

        if (onReading != null && distance(target, turn) > REST_BAND) {
            // Sensor time is on the boot clock; the frame timeline is on the monotonic one.
            val now = System.nanoTime()

            onReading(event.timestamp - SystemClock.elapsedRealtimeNanos() + now, now)
        }
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit

    /** Folds one reading into the neutral and works out where the studio should sit against it. */
    fun absorb(pose: Pose, seconds: Double) {
        val previous = previous
        var settled = neutral ?: pose

        if (previous != null && neutral != null) {
            stillFor = if (previous.angleTo(pose) / maxOf(seconds, 1e-4) < STILL_SPEED) {
                stillFor + seconds
            } else {
                0.0
            }

            // Put down somewhere new, so wherever it is becomes square. Still being moved, or only paused
            // mid-gesture, so neutral waits and the movement reads in full.
            if (stillFor >= DWELL) {
                settled = settled.blended(pose, minOf(seconds / SETTLE, 1.0))
            }
        } else {
            // The first reading is neutral outright, so the light starts square rather than drifting in from
            // wherever the phone happened to be picked up.
            settled = pose
        }

        neutral = settled
        this.previous = pose
        target = settled.turnTo(pose)

        if (isWorthWaking) onMove?.invoke()
    }

    private fun distance(a: Turn, b: Turn) = maxOf(abs(a.x - b.x), abs(a.y - b.y), abs(a.z - b.z))

    /** Where the studio is against where it is heading, on whichever axis is furthest off. */
    private val offset: Double get() = distance(target, turn)

    /**
     * Far enough out to be worth a frame.
     *
     * Deliberately much coarser than [REST_BAND]. Waking on the resting threshold meant waking on anything
     * at all: it works out to four hundredths of a degree of tilt, which is under the sensor's own noise
     * floor, so a phone merely held in a hand woke the renderer sixty times a second and redrew every coin
     * for as long as the card was open. Once awake, every reading past [REST_BAND] is drawn.
     */
    private val isWorthWaking: Boolean get() = offset > WAKE_BAND

    companion object {
        /**
         * The tilt that reaches the ends of the travel. Deliberately short of a right angle: nobody turns a
         * phone ninety degrees to look at it, and the whole range should be within a wrist's movement.
         */
        const val RANGE = PI / 4

        /**
         * How far the studio turns at the ends of the travel: as far as the phone itself, so the room stays
         * exactly where it is. It was half that while the room's own contrast was untamed; the lifted room
         * made the natural angle acceptable.
         */
        const val TRAVEL = RANGE

        /**
         * How quickly a phone that is being held still becomes the new neutral.
         *
         * Zero offset is where the studio sits as it was calibrated: square to the screen, which is where
         * the coins have most contrast. Any fixed idea of how a phone is held is wrong for somebody, so
         * neutral has to follow.
         *
         * It follows only while the phone is still. An average over the last few seconds cannot tell a
         * deliberate tilt from a new resting position, so it took both: a two second tilt dragged neutral
         * thirteen degrees, and putting the phone back where it started left a dozen degrees of light that
         * should not have been there. Worse, that bias ate one side of the travel.
         */
        const val SETTLE = 1.0

        /** Above this the phone is being moved rather than held, and neutral waits. */
        const val STILL_SPEED = 0.25

        /**
         * How long the phone has to be still before it counts as put down somewhere new. A hand pauses at
         * the end of a gesture before bringing the phone back, and without this that pause starts moving
         * neutral.
         */
        const val DWELL = 0.4

        /** Under sensor noise, so a field at rest does not keep redrawing for it. */
        private const val REST_BAND = 0.002

        /** A few readings' worth, in nanoseconds. */
        private const val LINGER = 100_000_000L

        /**
         * A degree of the phone's own tilt, converted into the studio's turn.
         *
         * An order of magnitude above hand tremor and sensor noise, and an order below a deliberate
         * movement, which is the whole width of the gap this has to sit in.
         */
        private val WAKE_BAND = TRAVEL / RANGE * (PI / 180)

        private const val INTERVAL_MICROSECONDS = 25_000
        private const val INTERVAL_SECONDS = 1.0 / 40
        private const val MIN_INTERVAL = 1.0 / 240
        private const val MAX_INTERVAL = 1.0 / 20
        private const val NANOSECONDS = 1_000_000_000.0

        /**
         * Scales a turn of `angle` into the travel, so the ends of the wrist's range reach the ends of the
         * studio's and no tilt can push it further.
         */
        fun scaled(angle: Double): Double = angle.coerceIn(-RANGE, RANGE) / RANGE * TRAVEL
    }
}
