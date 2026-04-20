package com.simply.droplets

import com.bitwig.extension.api.PlatformType
import com.bitwig.extension.controller.AutoDetectionMidiPortNamesList
import com.bitwig.extension.controller.ControllerExtensionDefinition
import com.bitwig.extension.controller.api.ControllerHost
import java.util.UUID

class DropletsExtensionDefinition : ControllerExtensionDefinition() {
    override fun getName() = "Simply Droplets"
    override fun getAuthor() = "Simply Chris"
    override fun getVersion() = "0.1.0"
    override fun getId(): UUID = DRIVER_ID
    override fun getHardwareVendor() = "Simply Chris"
    override fun getHardwareModel() = "Droplets"
    override fun getRequiredAPIVersion() = 18
    override fun getNumMidiInPorts() = 0
    override fun getNumMidiOutPorts() = 0

    override fun listAutoDetectionMidiPortNames(list: AutoDetectionMidiPortNamesList, platform: PlatformType) {}

    override fun createInstance(host: ControllerHost): DropletsExtension = DropletsExtension(this, host)

    companion object {
        private val DRIVER_ID: UUID = UUID.fromString("d70b1e55-5a3e-4d9c-9b3a-7a1f0d0a0001")
    }
}
