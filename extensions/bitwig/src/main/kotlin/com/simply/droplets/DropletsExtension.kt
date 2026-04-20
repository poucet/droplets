package com.simply.droplets

import com.bitwig.extension.callback.StringValueChangedCallback
import com.bitwig.extension.controller.ControllerExtension
import com.bitwig.extension.controller.api.ControllerHost
import com.bitwig.extension.controller.api.Device
import com.bitwig.extension.controller.api.DrumPad
import com.bitwig.extension.controller.api.EnumValue
import com.bitwig.extension.controller.api.StringValue
import com.bitwig.extension.controller.api.Track
import com.bitwig.extension.controller.api.TrackBank

private const val NUM_TRACKS = 32
private const val DEVICES_PER_TRACK = 16
private const val DRUM_PADS = 128
private const val DEVICES_PER_PAD = 8
private const val REBUILD_DEBOUNCE_MS = 150L
private const val DRUM_ROOT_NOTE = 36
private val INSTANCE_ID_REGEX = Regex("^droplets-[0-9a-f]+$")

class DropletsExtension(
    definition: DropletsExtensionDefinition,
    private val host: ControllerHost,
) : ControllerExtension(definition, host) {

    private lateinit var client: DropletsClient
    private lateinit var trackBank: TrackBank
    private val renamedInstances = HashSet<String>()

    /** Per-device param-id → current displayed-value cache. Used to find the Droplets instance-ID param. */
    private val deviceDisplays = HashMap<Device, MutableMap<String, String>>()

    private var rebuildScheduled = false

    override fun init() {
        client = DropletsClient(::log)
        client.connectWebSocket()

        trackBank = host.createTrackBank(NUM_TRACKS, 0, 0)
        for (t in 0 until NUM_TRACKS) wireTrack(trackBank.getItemAt(t) as Track)

        scheduleRebuild()
        log("Droplets extension initialized — watching $NUM_TRACKS tracks")
    }

    override fun flush() {}

    override fun exit() {
        client.closeWebSocket()
    }

    // --- Wiring ---------------------------------------------------------

    private fun wireTrack(track: Track) {
        track.exists().markInterested()
        track.position().markInterested()
        subscribeString(track.name())
        val devices = track.createDeviceBank(DEVICES_PER_TRACK)
        for (d in 0 until DEVICES_PER_TRACK) wireDevice(devices.getItemAt(d) as Device)
    }

    private fun wireDevice(device: Device) {
        device.exists().markInterested()
        device.isPlugin().markInterested()
        device.hasDrumPads().markInterested()
        subscribeString(device.name())
        subscribeString(device.presetName())
        subscribeEnum(device.deviceType())

        deviceDisplays[device] = HashMap()
        device.addDirectParameterValueDisplayObserver(32) { id, value ->
            if (id != null && value != null) {
                deviceDisplays[device]?.put(id, value)
                if (INSTANCE_ID_REGEX.matches(value)) scheduleRebuild()
            }
        }

        val padBank = device.createDrumPadBank(DRUM_PADS)
        for (p in 0 until DRUM_PADS) wirePad(padBank.getItemAt(p) as DrumPad)
    }

    private fun wirePad(pad: DrumPad) {
        pad.exists().markInterested()
        subscribeString(pad.name())
        val padDevices = pad.createDeviceBank(DEVICES_PER_PAD)
        for (d in 0 until DEVICES_PER_PAD) wirePadDevice(padDevices.getItemAt(d) as Device)
    }

    private fun wirePadDevice(device: Device) {
        device.exists().markInterested()
        device.isPlugin().markInterested()
        subscribeString(device.name())
        subscribeString(device.presetName())
        subscribeEnum(device.deviceType())
    }

    private fun subscribeString(v: StringValue) {
        v.markInterested()
        v.addValueObserver(StringValueChangedCallback { scheduleRebuild() })
    }

    private fun subscribeEnum(v: EnumValue) {
        v.markInterested()
        v.addValueObserver(StringValueChangedCallback { scheduleRebuild() })
    }

    // --- Rebuild + POST -------------------------------------------------

    private fun scheduleRebuild() {
        if (rebuildScheduled) return
        rebuildScheduled = true
        host.scheduleTask({
            rebuildScheduled = false
            try {
                rebuildAndPost()
            } catch (e: Throwable) {
                log("rebuild failed: ${e.message}")
            }
        }, REBUILD_DEBOUNCE_MS)
    }

    private fun rebuildAndPost() {
        val tracks = ArrayList<Map<String, Any?>>()
        for (t in 0 until NUM_TRACKS) {
            val track = trackBank.getItemAt(t) as Track
            if (!track.exists().get()) continue
            val trackName: String = track.name().get()
            val trackDevices = track.createDeviceBank(DEVICES_PER_TRACK)
            val devicesJson = ArrayList<Map<String, Any?>>()
            var instanceId: String? = null
            for (d in 0 until DEVICES_PER_TRACK) {
                val device = trackDevices.getItemAt(d) as Device
                if (!device.exists().get()) continue
                if (instanceId == null) findInstanceId(device)?.let { instanceId = it }
                devicesJson.add(encodeDevice(device))
            }
            tracks.add(linkedMapOf(
                "track_name" to trackName,
                "droplets_instance_id" to instanceId,
                "devices" to devicesJson,
            ))

            val id = instanceId
            if (id != null && renamedInstances.add(id)) {
                client.postRenameInstance(id, trackName)
            }
        }
        client.postProjectLayout(Json.encode(mapOf("tracks" to tracks)))
    }

    private fun findInstanceId(device: Device): String? {
        val displays = deviceDisplays[device] ?: return null
        for (v in displays.values) if (INSTANCE_ID_REGEX.matches(v)) return v
        return null
    }

    private fun encodeDevice(device: Device): Map<String, Any?> {
        val name: String = device.name().get()
        val preset: String? = device.presetName().get().takeIf { it.isNotBlank() }
        val isPlugin = device.isPlugin().get()
        val vendor = if (isPlugin) null else "Bitwig"

        if (device.hasDrumPads().get()) {
            val padBank = device.createDrumPadBank(DRUM_PADS)
            val pads = ArrayList<Map<String, Any?>>()
            for (p in 0 until DRUM_PADS) {
                val pad = padBank.getItemAt(p) as DrumPad
                if (!pad.exists().get()) continue
                val padDevices = ArrayList<Map<String, Any?>>()
                val nestedBank = pad.createDeviceBank(DEVICES_PER_PAD)
                for (i in 0 until DEVICES_PER_PAD) {
                    val nd = nestedBank.getItemAt(i) as Device
                    if (!nd.exists().get()) continue
                    padDevices.add(encodeDevice(nd))
                }
                if (padDevices.isEmpty()) continue
                pads.add(linkedMapOf(
                    "note" to (DRUM_ROOT_NOTE + p),
                    "name" to pad.name().get(),
                    "devices" to padDevices,
                ))
            }
            return linkedMapOf("type" to "drum_machine", "name" to name, "pads" to pads)
        }

        val kind = when (device.deviceType().get()) {
            "instrument" -> "instrument"
            "audio_effect" -> "effect"
            "note_effect" -> "effect"
            else -> "unknown"
        }
        val out = linkedMapOf<String, Any?>("type" to kind, "name" to name)
        if (vendor != null) out["vendor"] = vendor
        if (preset != null) out["preset_name"] = preset
        return out
    }

    private fun log(msg: String) = host.println("[droplets] $msg")
}
