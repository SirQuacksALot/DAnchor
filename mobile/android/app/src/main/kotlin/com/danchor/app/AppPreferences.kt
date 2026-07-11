package com.danchor.app

import android.content.Context
import android.os.Build
import androidx.core.content.edit
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.security.SecureRandom
import java.util.UUID

enum class ThemeMode {
    LIGHT,
    DARK,
    SYSTEM,
}

/** Thin synchronous SharedPreferences wrapper. Saved connections are a small
 * flat list, so a hand-rolled org.json encoding (built into the Android SDK)
 * is simpler than pulling in Room/DataStore/kotlinx.serialization for this. */
class AppPreferences(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    var themeMode: ThemeMode
        get() = ThemeMode.entries.find { it.name == prefs.getString(KEY_THEME, null) } ?: ThemeMode.SYSTEM
        set(value) = prefs.edit { putString(KEY_THEME, value.name) }

    var autoReloadEnabled: Boolean
        get() = prefs.getBoolean(KEY_AUTO_RELOAD, true)
        set(value) = prefs.edit { putBoolean(KEY_AUTO_RELOAD, value) }

    var usbHintHidden: Boolean
        get() = prefs.getBoolean(KEY_USB_HINT_HIDDEN, false)
        set(value) = prefs.edit { putBoolean(KEY_USB_HINT_HIDDEN, value) }

    /** Whether this tablet auto-accepts connections from any desktop, not
     * just ones presenting the matching [pairingSecret] - Bluetooth-style
     * discoverability. Not yet enforced over the wire (no pairing protocol
     * exists yet, see .ai/tasks.toon); this just persists the UI toggle so
     * that protocol has something to read once it lands. */
    var visibleForAll: Boolean
        get() = prefs.getBoolean(KEY_VISIBLE_FOR_ALL, false)
        set(value) = prefs.edit { putBoolean(KEY_VISIBLE_FOR_ALL, value) }

    var deviceProfileName: String
        get() = prefs.getString(KEY_DEVICE_PROFILE_NAME, null) ?: Build.MODEL
        set(value) = prefs.edit { putString(KEY_DEVICE_PROFILE_NAME, value) }

    /** Stable identity for this device, independent of its display name
     * (which the user can freely rename) - generated once on first read if
     * absent. Not yet used anywhere over the wire (no pairing protocol
     * exists yet, see .ai/tasks.toon); this is the identity that protocol
     * will eventually carry. */
    val deviceId: String
        get() {
            val existing = prefs.getString(KEY_DEVICE_ID, null)
            if (existing != null) return existing
            val generated = UUID.randomUUID().toString()
            prefs.edit { putString(KEY_DEVICE_ID, generated) }
            return generated
        }

    /** This device's trust secret - shared manually across a person's own
     * devices so they can later recognize each other without an account
     * system. Generated once on first read if absent. */
    var pairingSecret: String
        get() {
            val existing = prefs.getString(KEY_PAIRING_SECRET, null)
            if (existing != null) return existing
            val generated = generateSecret()
            prefs.edit { putString(KEY_PAIRING_SECRET, generated) }
            return generated
        }
        set(value) = prefs.edit { putString(KEY_PAIRING_SECRET, value) }

    var savedConnections: List<SavedConnection>
        get() {
            val raw = prefs.getString(KEY_SAVED_CONNECTIONS, null) ?: return emptyList()
            return try {
                val array = JSONArray(raw)
                (0 until array.length()).map { i ->
                    val obj = array.getJSONObject(i)
                    SavedConnection(
                        id = obj.getString("id"),
                        name = obj.getString("name"),
                        host = obj.getString("host"),
                        port = obj.getInt("port"),
                        lastConnectedAtMs = obj.getLong("lastConnectedAtMs"),
                    )
                }
            } catch (e: JSONException) {
                emptyList()
            }
        }
        set(value) {
            val array = JSONArray()
            value.forEach { connection ->
                array.put(
                    JSONObject().apply {
                        put("id", connection.id)
                        put("name", connection.name)
                        put("host", connection.host)
                        put("port", connection.port)
                        put("lastConnectedAtMs", connection.lastConnectedAtMs)
                    },
                )
            }
            prefs.edit { putString(KEY_SAVED_CONNECTIONS, array.toString()) }
        }

    /** Forces a fresh secret regardless of whether one already exists (the
     * [pairingSecret] getter only generates one when absent). */
    fun regenerateSecret(): String {
        val generated = generateSecret()
        pairingSecret = generated
        return generated
    }

    /** Upserts by [SavedConnection.id], keeping the rest of the list untouched. */
    fun upsertSavedConnection(connection: SavedConnection) {
        savedConnections = savedConnections.filter { it.id != connection.id } + connection
    }

    /** Removes a saved connection by id ("forget connection"). */
    fun removeSavedConnection(id: String) {
        savedConnections = savedConnections.filter { it.id != id }
    }

    private fun generateSecret(): String {
        val bytes = ByteArray(32)
        SecureRandom().nextBytes(bytes)
        return bytes.joinToString("") { "%02x".format(it) }
    }

    private companion object {
        const val PREFS_NAME = "danchor_prefs"
        const val KEY_THEME = "theme_mode"
        const val KEY_AUTO_RELOAD = "auto_reload"
        const val KEY_USB_HINT_HIDDEN = "usb_hint_hidden"
        const val KEY_VISIBLE_FOR_ALL = "visible_for_all"
        const val KEY_DEVICE_PROFILE_NAME = "device_profile_name"
        const val KEY_DEVICE_ID = "device_id"
        const val KEY_PAIRING_SECRET = "pairing_secret"
        const val KEY_SAVED_CONNECTIONS = "saved_connections"
    }
}
