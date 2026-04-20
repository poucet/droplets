package com.simply.droplets

import com.bitwig.extension.callback.BooleanValueChangedCallback
import com.bitwig.extension.callback.EnumValueChangedCallback
import com.bitwig.extension.callback.StringArrayValueChangedCallback
import com.bitwig.extension.callback.StringValueChangedCallback
import com.bitwig.extension.controller.ControllerExtension
import com.bitwig.extension.controller.api.ControllerHost
import com.bitwig.extension.controller.api.Device
import com.bitwig.extension.controller.api.DeviceBank
import com.bitwig.extension.controller.api.DirectParameterValueDisplayObserver
import com.bitwig.extension.controller.api.DrumPad
import com.bitwig.extension.controller.api.DrumPadBank
import com.bitwig.extension.controller.api.EnumValue
import com.bitwig.extension.controller.api.StringValue
import com.bitwig.extension.controller.api.Track
import com.bitwig.extension.controller.api.TrackBank
import java.util.UUID

private const val NUM_TRACKS = 32
private const val DEVICES_PER_TRACK = 16
// Drum machines usually hold ~16 samples, max ~32, clustered in a ~2-octave range
// around C0–C3 (GM convention: kick=36/C1, snare=38/D1, hat=42/F#1). Allocate 32
// pad slots scrolled to MIDI 24 (C0 in Bitwig's C3=60 notation) — covers MIDI 24–55
// (C0 to G2), which catches every realistic kit placement. Pads outside that range
// won't surface in the layout.
private const val DRUM_PADS = 32
private const val DRUM_BANK_SCROLL = 24
private const val DEVICES_PER_PAD = 2
private const val REBUILD_DEBOUNCE_MS = 150L

// Bitwig's native Drum Machine UUID. Published in the community extension library
// (novation.commonsmk3.SpecialDevices). Used with `createBitwigDeviceMatcher` to
// narrow a filtered device bank to drum machines only — we attach the 32-pad bank
// to the filtered slot, not to every device slot (that OOMs the JVM).
private val DRUM_MACHINE_UUID: UUID = UUID.fromString("8ea97e45-0255-40fd-bc7e-94419741e9d1")

// Cached display strings matching this shape are the Droplets instance ID.
private val INSTANCE_ID_REGEX = Regex("^droplets-[0-9a-f]+$")

class DropletsExtension(
    definition: DropletsExtensionDefinition,
    private val host: ControllerHost,
) : ControllerExtension(definition, host) {

    private lateinit var client: DropletsClient
    private lateinit var trackBank: TrackBank
    private val renamedInstances = HashSet<String>()

    /** Instance IDs we've already announced in the log. Keeps detection logs non-spammy. */
    private val announcedInstances = HashSet<String>()

    /** Per-device param-id → displayed-value cache. Scanned for the Droplets ID pattern. */
    private val deviceDisplays = HashMap<Device, MutableMap<String, String>>()

    /** Per-track device bank, created at init. Rebuilds reuse this — creating
     *  a fresh DeviceBank during rebuild throws "can only be called during
     *  driver initialization". */
    private val trackDeviceBanks = HashMap<Track, DeviceBank>()

    /** Per-track drum pad bank, taken from a device-matcher-filtered bank (1 drum machine per track). */
    private val trackDrumPadBanks = HashMap<Track, DrumPadBank>()

    /** Per-pad nested device bank, created at init. */
    private val padDeviceBanks = HashMap<DrumPad, DeviceBank>()

    private var rebuildScheduled = false

    /**
     * Mirror of `host.println` to a file so logs are visible without the
     * Bitwig Controller Script Console (which some Bitwig versions don't
     * expose a menu entry for). Path matches the plugin side's convention
     * so both logs can be tailed from the same dir.
     */
    private val logFile: java.io.File =
        java.io.File(System.getProperty("user.home"), "droplets_bitwig.log")

    override fun init() {
        client = DropletsClient(::log)

        trackBank = host.createTrackBank(NUM_TRACKS, 0, 0)
        for (t in 0 until NUM_TRACKS) wireTrack(trackBank.getItemAt(t) as Track)

        scheduleRebuild()
        log("Droplets extension initialized — watching $NUM_TRACKS tracks")
    }

    override fun flush() {}

    override fun exit() {
        client.shutdown()
    }

    // --- Wiring (all done at init — Bitwig rejects observer registration afterwards) ---

    private fun wireTrack(track: Track) {
        track.exists().markInterested()
        // Trigger a rebuild when a track appears/disappears. Without this we
        // only pick up tracks via their name observer firing — which for the
        // INITIAL population of the bank can land AFTER our first scheduled
        // rebuild, making the extension silently report "0 tracks" forever.
        track.exists().addValueObserver(BooleanValueChangedCallback { scheduleRebuild() })
        track.position().markInterested()
        subscribeString(track.name())

        // Full device bank: all devices on the track. Cheap per-device observers for
        // track info + direct-parameter observers for Droplets identification.
        // Cached for rebuilds — Bitwig only allows createDeviceBank during init().
        val devices = track.createDeviceBank(DEVICES_PER_TRACK)
        trackDeviceBanks[track] = devices
        for (d in 0 until DEVICES_PER_TRACK) wireDevice(devices.getItemAt(d) as Device)

        // Drum-machine-filtered bank (1 slot per track). The filter keeps the drum pad
        // bank attached to at most one device per track — 32 pad proxies × 2 nested
        // devs = 64 pad proxies per track, rather than 512 × 32 × 2 = 32K for the
        // unfiltered case.
        val drumMatcher = host.createBitwigDeviceMatcher(DRUM_MACHINE_UUID)
        val drumBank = track.createDeviceBank(1)
        drumBank.setDeviceMatcher(drumMatcher)
        val drumDev = drumBank.getItemAt(0) as Device
        drumDev.exists().markInterested()
        drumDev.exists().addValueObserver(BooleanValueChangedCallback { scheduleRebuild() })
        val padBank = drumDev.createDrumPadBank(DRUM_PADS)
        // Scroll to MIDI 24 so pad index + 24 == MIDI note; lets us allocate a small bank
        // (32 slots) while still covering the full typical drum-kit range (C0–G2).
        padBank.scrollPosition().set(DRUM_BANK_SCROLL)
        trackDrumPadBanks[track] = padBank
        for (p in 0 until DRUM_PADS) wirePad(padBank.getItemAt(p) as DrumPad)
    }

    private fun wireDevice(device: Device) {
        device.exists().markInterested()
        device.isPlugin().markInterested()
        device.hasDrumPads().markInterested()
        subscribeString(device.name())
        subscribeString(device.presetName())
        subscribeEnum(device.deviceType())

        // Direct-parameter observers fire the display string of every parameter. We only
        // care about the Droplets instance-ID param (display matches `droplets-[hex]+`),
        // but we can't pre-filter by plugin identity (no CLAP device matcher exists; the
        // VST3 UID isn't pinned on the Rust side). Pay the observer cost on all devices.
        // Bitwig requires the ID observer to be registered BEFORE the display observer —
        // the holder pattern resolves that circular dependency.
        deviceDisplays[device] = HashMap()
        val displayObsHolder = arrayOfNulls<DirectParameterValueDisplayObserver>(1)

        device.addDirectParameterIdObserver(StringArrayValueChangedCallback { raw ->
            (raw as? Array<*>)
                ?.filterIsInstance<String>()
                ?.toTypedArray()
                ?.let { displayObsHolder[0]?.setObservedParameterIds(it) }
        })
        displayObsHolder[0] = device.addDirectParameterValueDisplayObserver(32) { id, value ->
            if (id != null && value != null) {
                deviceDisplays[device]?.put(id, value)
                if (INSTANCE_ID_REGEX.matches(value)) scheduleRebuild()
            }
        }
    }

    private fun wirePad(pad: DrumPad) {
        pad.exists().markInterested()
        pad.exists().addValueObserver(BooleanValueChangedCallback { scheduleRebuild() })
        pad.name().markInterested()
        pad.name().addValueObserver(StringValueChangedCallback { scheduleRebuild() })
        val bank = pad.createDeviceBank(DEVICES_PER_PAD)
        padDeviceBanks[pad] = bank
        for (d in 0 until DEVICES_PER_PAD) {
            val dev = bank.getItemAt(d) as Device
            dev.exists().markInterested()
            dev.exists().addValueObserver(BooleanValueChangedCallback { scheduleRebuild() })
            dev.name().markInterested()
            dev.name().addValueObserver(StringValueChangedCallback { scheduleRebuild() })
            dev.presetName().markInterested()
            dev.isPlugin().markInterested()
            dev.deviceType().markInterested()
        }
    }

    private fun subscribeString(v: StringValue) {
        v.markInterested()
        v.addValueObserver(StringValueChangedCallback { scheduleRebuild() })
    }

    private fun subscribeEnum(v: EnumValue) {
        v.markInterested()
        v.addValueObserver(EnumValueChangedCallback { scheduleRebuild() })
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
        var trackCount = 0
        var dropletsCount = 0
        for (t in 0 until NUM_TRACKS) {
            val track = trackBank.getItemAt(t) as Track
            if (!track.exists().get()) continue
            trackCount++
            val trackName: String = track.name().get()
            val trackDevices = trackDeviceBanks[track] ?: continue
            val devicesJson = ArrayList<Map<String, Any?>>()
            var instanceId: String? = null
            for (d in 0 until DEVICES_PER_TRACK) {
                val device = trackDevices.getItemAt(d) as Device
                if (!device.exists().get()) continue
                if (instanceId == null) findInstanceId(device)?.let { instanceId = it }
                devicesJson.add(encodeDevice(device, track))
            }
            if (instanceId != null) {
                dropletsCount++
                val id = instanceId
                if (announcedInstances.add(id)) {
                    log("detected Droplets instance '$id' on track '$trackName'")
                }
                if (renamedInstances.add(id)) {
                    log("→ rename $id to '$trackName'")
                    client.postRenameInstance(id, trackName)
                }
            }
            tracks.add(linkedMapOf(
                "track_name" to trackName,
                "droplets_instance_id" to instanceId,
                "devices" to devicesJson,
            ))
        }

        val hasAnyDroplets = dropletsCount > 0
        client.setWebSocketDesired(hasAnyDroplets)
        // Log every rebuild unconditionally. Silent empty rebuilds made it
        // impossible to tell whether the extension was running or not.
        log("rebuild: $trackCount tracks, $dropletsCount Droplets")
        if (hasAnyDroplets) {
            log("  → POSTing layout (${tracks.size} tracks)")
            client.postProjectLayout(Json.encode(mapOf("tracks" to tracks)))
        }
    }

    private fun findInstanceId(device: Device): String? {
        val displays = deviceDisplays[device] ?: return null
        for (v in displays.values) if (INSTANCE_ID_REGEX.matches(v)) return v
        return null
    }

    private fun encodeDevice(device: Device, track: Track): Map<String, Any?> {
        val name: String = device.name().get()
        val preset: String? = device.presetName().get().takeIf { it.isNotBlank() }
        val isPlugin = device.isPlugin().get()
        val vendor = if (isPlugin) null else "Bitwig"

        if (device.hasDrumPads().get()) {
            // Pads live on the track's filtered bank (attached at init). A track with
            // multiple drum machines only gets pads enumerated on the first one.
            val padBank = trackDrumPadBanks[track]
            if (padBank != null) return encodeDrumMachine(name, padBank)
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

    private fun encodeDrumMachine(name: String, padBank: DrumPadBank): Map<String, Any?> {
        val pads = ArrayList<Map<String, Any?>>()
        for (p in 0 until DRUM_PADS) {
            val pad = padBank.getItemAt(p) as DrumPad
            if (!pad.exists().get()) continue
            val nestedBank = padDeviceBanks[pad] ?: continue
            val padDevices = ArrayList<Map<String, Any?>>()
            for (i in 0 until DEVICES_PER_PAD) {
                val nd = nestedBank.getItemAt(i) as Device
                if (!nd.exists().get()) continue
                padDevices.add(encodePadDevice(nd))
            }
            if (padDevices.isEmpty()) continue
            pads.add(linkedMapOf(
                "note" to (DRUM_BANK_SCROLL + p),
                "name" to pad.name().get(),
                "devices" to padDevices,
            ))
        }
        return linkedMapOf("type" to "drum_machine", "name" to name, "pads" to pads)
    }

    private fun encodePadDevice(device: Device): Map<String, Any?> {
        val name = device.name().get()
        val preset = device.presetName().get().takeIf { it.isNotBlank() }
        val isPlugin = device.isPlugin().get()
        val kind = when (device.deviceType().get()) {
            "instrument" -> "instrument"
            "audio_effect" -> "effect"
            "note_effect" -> "effect"
            else -> "unknown"
        }
        val out = linkedMapOf<String, Any?>("type" to kind, "name" to name)
        if (!isPlugin) out["vendor"] = "Bitwig"
        if (preset != null) out["preset_name"] = preset
        return out
    }

    private fun log(msg: String) {
        host.println("[droplets] $msg")
        // Also tee to a file so `tail -f ~/droplets_bitwig.log` works even
        // when the Controller Script Console isn't reachable from the menu.
        try {
            val ts = java.time.LocalTime.now().toString()
            logFile.appendText("[$ts] $msg\n")
        } catch (_: Throwable) {
            // Best-effort — never let logging break the extension.
        }
    }
}
