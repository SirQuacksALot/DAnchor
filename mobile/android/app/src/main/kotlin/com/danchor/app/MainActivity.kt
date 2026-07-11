package com.danchor.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.appcompat.app.AppCompatActivity
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material.ripple.RippleAlpha
import androidx.compose.material3.LocalRippleConfiguration
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RippleConfiguration
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.core.content.ContextCompat

private const val SERVICE_TYPE = "_danchor._tcp."

// UsbManager.ACTION_USB_STATE / USB_CONNECTED and Settings.ACTION_TETHER_SETTINGS
// are real, stable, widely-used broadcasts/intents but aren't part of the
// public SDK (no compile-time constant), so their literal string values are
// hardcoded here - verified live against the real tablet (the tether
// settings intent opens exactly the right screen).
private const val ACTION_USB_STATE = "android.hardware.usb.action.USB_STATE"
private const val EXTRA_USB_CONNECTED = "connected"
private const val ACTION_TETHER_SETTINGS = "android.settings.TETHER_SETTINGS"

class MainActivity : AppCompatActivity() {
    private lateinit var nsdManager: NsdManager
    private lateinit var connectivityManager: ConnectivityManager
    private lateinit var preferences: AppPreferences
    private var discoveryListener: NsdManager.DiscoveryListener? = null
    private var usbStateReceiver: BroadcastReceiver? = null

    // Whether a USB cable is physically plugged in (regardless of whether
    // tethering is enabled), surfaced to Compose so it can prompt the user
    // to enable tethering when the cable's in but unused - a cable
    // connection is faster and far more stable than WiFi (no interference,
    // no roaming/power-save latency spikes), so it's worth nudging toward.
    private var usbCableConnected by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        nsdManager = getSystemService(NsdManager::class.java)
        connectivityManager = getSystemService(ConnectivityManager::class.java)
        preferences = AppPreferences(this)

        setContent {
            // Temporary Module 2b groundwork test hooks - see
            // VideoTestPlayer.kt (local-file decode test) and
            // LiveMirrorPlayer.kt (live network mirror test). A marker file
            // containing "host:port" switches to the live variant; a
            // populated test_frames directory switches to the file-based one.
            val mirrorTargetFile = java.io.File(getExternalFilesDir(null), "mirror_target.txt")
            if (mirrorTargetFile.exists()) {
                val (host, port) = mirrorTargetFile.readText().trim().split(":")
                LiveMirrorPlayer(host, port.toInt(), preferences.pairingSecret)
                return@setContent
            }

            val testFramesDir = java.io.File(getExternalFilesDir(null), "test_frames")
            if (testFramesDir.listFiles()?.isNotEmpty() == true) {
                VideoTestPlayer(testFramesDir)
                return@setContent
            }

            var themeMode by remember { mutableStateOf(preferences.themeMode) }
            var autoReloadEnabled by remember { mutableStateOf(preferences.autoReloadEnabled) }
            var usbHintHidden by remember { mutableStateOf(preferences.usbHintHidden) }
            var visibleForAll by remember { mutableStateOf(preferences.visibleForAll) }
            var savedConnections by remember { mutableStateOf(preferences.savedConnections) }
            var deviceProfileName by remember { mutableStateOf(preferences.deviceProfileName) }
            var pairingSecret by remember { mutableStateOf(preferences.pairingSecret) }

            val useDarkColors =
                when (themeMode) {
                    ThemeMode.LIGHT -> false
                    ThemeMode.DARK -> true
                    ThemeMode.SYSTEM -> isSystemInDarkTheme()
                }

            MaterialTheme(
                colorScheme = if (useDarkColors) DAnchorDarkColorScheme else DAnchorLightColorScheme,
                typography = DAnchorTypography,
            ) {
                // Sebastian asked for the default press/hover ripple (the
                // gray/translucent-white flash Material draws on every
                // clickable) to be gone app-wide - zeroing every RippleAlpha
                // channel here does that without touching each individual
                // Button/Card call site's indication.
                CompositionLocalProvider(
                    LocalRippleConfiguration provides
                        RippleConfiguration(color = Color.Unspecified, rippleAlpha = RippleAlpha(0f, 0f, 0f, 0f)),
                ) {
                    DAnchorApp(
                        onStartDiscovery = { onFound, onLost -> startDiscovery(onFound, onLost) },
                        onStopDiscovery = { stopDiscovery() },
                        onScanFallback = { scanSubnetForDesktops(this) },
                        usbCableConnected = usbCableConnected,
                        onCheckUsbTethering = { findUsbNetwork(connectivityManager) != null },
                        onCheckWifiConnected = { isWifiConnected(connectivityManager) },
                        onOpenTetherSettings = { startActivity(Intent(ACTION_TETHER_SETTINGS)) },
                        themeMode = themeMode,
                        onThemeModeChange = {
                            themeMode = it
                            preferences.themeMode = it
                        },
                        autoReloadEnabled = autoReloadEnabled,
                        onAutoReloadEnabledChange = {
                            autoReloadEnabled = it
                            preferences.autoReloadEnabled = it
                        },
                        usbHintHidden = usbHintHidden,
                        onUsbHintHiddenChange = {
                            usbHintHidden = it
                            preferences.usbHintHidden = it
                        },
                        visibleForAll = visibleForAll,
                        onVisibleForAllChange = {
                            visibleForAll = it
                            preferences.visibleForAll = it
                        },
                        savedConnections = savedConnections,
                        onConnectionPinged = { connection ->
                            preferences.upsertSavedConnection(connection)
                            savedConnections = preferences.savedConnections
                        },
                        onForgetConnection = { id ->
                            preferences.removeSavedConnection(id)
                            savedConnections = preferences.savedConnections
                        },
                        deviceProfileName = deviceProfileName,
                        onDeviceProfileNameChange = {
                            deviceProfileName = it
                            preferences.deviceProfileName = it
                        },
                        deviceId = preferences.deviceId,
                        pairingSecret = pairingSecret,
                        onRegenerateSecret = { pairingSecret = preferences.regenerateSecret() },
                        onAdoptSecret = {
                            pairingSecret = it
                            preferences.pairingSecret = it
                        },
                    )
                }
            }
        }
    }

    override fun onStart() {
        super.onStart()

        val receiver =
            object : BroadcastReceiver() {
                override fun onReceive(
                    context: Context,
                    intent: Intent,
                ) {
                    usbCableConnected = intent.getBooleanExtra(EXTRA_USB_CONNECTED, false)
                }
            }
        usbStateReceiver = receiver
        ContextCompat.registerReceiver(
            this,
            receiver,
            IntentFilter(ACTION_USB_STATE),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
    }

    override fun onStop() {
        usbStateReceiver?.let { unregisterReceiver(it) }
        usbStateReceiver = null
        super.onStop()
    }

    private fun startDiscovery(
        onFound: (DiscoveredDesktop) -> Unit,
        onLost: (String) -> Unit,
    ) {
        val listener =
            object : NsdManager.DiscoveryListener {
                override fun onDiscoveryStarted(serviceType: String) {}

                override fun onDiscoveryStopped(serviceType: String) {}

                override fun onStartDiscoveryFailed(
                    serviceType: String,
                    errorCode: Int,
                ) {
                    nsdManager.stopServiceDiscovery(this)
                }

                override fun onStopDiscoveryFailed(
                    serviceType: String,
                    errorCode: Int,
                ) {}

                override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                    nsdManager.resolveService(
                        serviceInfo,
                        object : NsdManager.ResolveListener {
                            override fun onResolveFailed(
                                serviceInfo: NsdServiceInfo,
                                errorCode: Int,
                            ) {}

                            override fun onServiceResolved(serviceInfo: NsdServiceInfo) {
                                val host = serviceInfo.host ?: return
                                onFound(
                                    DiscoveredDesktop(
                                        serviceInfo.serviceName,
                                        host,
                                        serviceInfo.port,
                                        DesktopSource.Mdns,
                                    ),
                                )
                            }
                        },
                    )
                }

                override fun onServiceLost(serviceInfo: NsdServiceInfo) {
                    onLost(serviceInfo.serviceName)
                }
            }
        discoveryListener = listener
        nsdManager.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
    }

    private fun stopDiscovery() {
        discoveryListener?.let { nsdManager.stopServiceDiscovery(it) }
        discoveryListener = null
    }

    override fun onDestroy() {
        stopDiscovery()
        super.onDestroy()
    }
}
