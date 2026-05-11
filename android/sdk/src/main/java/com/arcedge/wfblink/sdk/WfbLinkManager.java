package com.arcedge.wfblink.sdk;

/** Blocking SDK facade for Android USBHost WFB Link sessions. */
public final class WfbLinkManager {
    static {
        System.loadLibrary("wfb_android");
    }

    public WfbManagedStreamsResult runManagedStreamsBlocking(WfbManagedStreamsConfig config)
            throws WfbLinkException {
        validateManagedStreamsConfig(config);
        String json =
                WfbLinkNative.runManagedStreams(
                        config.usb.connection,
                        config.usb.bulkInEndpointObject,
                        config.usb.bulkOutEndpointObject,
                        config.usb.fd,
                        config.usb.vid,
                        config.usb.pid,
                        config.usb.interfaceNumber,
                        config.usb.bulkInEndpoint,
                        config.usb.bulkOutEndpoint,
                        config.usb.bulkOutEndpointCount,
                        config.channelNumber,
                        config.timeoutMs,
                        config.nativeLibraryDir,
                        config.workingDir,
                        config.keyPath,
                        config.firmwarePath,
                        config.macTablePath,
                        config.bbTablePath,
                        config.rfTablePath,
                        config.durationMs,
                        config.payloadCount,
                        config.linkId,
                        config.uplinkRadioPort,
                        config.downlinkRadioPort,
                        config.runtimeBindPort,
                        config.txBindPort,
                        config.rawTxPort,
                        config.rxAggregatorPort,
                        config.rawRxPort,
                        config.rawPayloadBytes);
        return WfbManagedStreamsResult.fromJson(json);
    }

    private static void validateManagedStreamsConfig(WfbManagedStreamsConfig config)
            throws WfbLinkException {
        if (config == null) {
            throw new WfbLinkException("android_sdk_config_missing", "config is required");
        }
        if (config.usb == null
                || config.usb.connection == null
                || config.usb.bulkInEndpointObject == null
                || config.usb.bulkOutEndpointObject == null) {
            throw new WfbLinkException(
                    "android_sdk_usb_handoff_missing",
                    "UsbDeviceConnection and bulk endpoint objects are required");
        }
        requireString("native_library_dir", config.nativeLibraryDir);
        requireString("working_dir", config.workingDir);
        requireString("key_path", config.keyPath);
        requireString("firmware_path", config.firmwarePath);
        requireString("mac_table_path", config.macTablePath);
        requireString("bb_table_path", config.bbTablePath);
        requireString("rf_table_path", config.rfTablePath);
        if (!config.keyFileExists()) {
            throw new WfbLinkException(
                    "android_sdk_key_missing",
                    "paired GS key is not readable at " + config.keyPath);
        }
        requirePositive("channel_number", config.channelNumber);
        requirePositive("timeout_ms", config.timeoutMs);
        requirePositive("duration_ms", config.durationMs);
        if (config.payloadCount < 0) {
            throw new WfbLinkException("android_sdk_invalid_payload_count", "payload_count < 0");
        }
        requirePositive("raw_payload_bytes", config.rawPayloadBytes);
    }

    private static void requireString(String field, String value) throws WfbLinkException {
        if (value == null || value.length() == 0) {
            throw new WfbLinkException("android_sdk_" + field + "_missing", field + " is required");
        }
    }

    private static void requirePositive(String field, int value) throws WfbLinkException {
        if (value <= 0) {
            throw new WfbLinkException("android_sdk_invalid_" + field, field + " must be positive");
        }
    }
}
