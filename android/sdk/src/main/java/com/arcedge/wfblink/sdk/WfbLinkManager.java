package com.arcedge.wfblink.sdk;

import java.util.HashSet;
import java.util.Set;
import java.util.concurrent.ExecutorService;

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
                        config.rawPayloadBytes,
                        config.txPayloadIntervalMs,
                        config.validationTrafficEnabled,
                        config.txBandwidthMhz,
                        config.txMcs,
                        config.txFecK,
                        config.txFecN);
        return WfbManagedStreamsResult.fromJson(json);
    }

    public WfbManagedStreamsSession startManagedStreams(
            WfbManagedStreamsConfig config, ExecutorService executor) throws WfbLinkException {
        return startManagedStreams(config, executor, null);
    }

    public WfbManagedStreamsSession startManagedStreams(
            WfbManagedStreamsConfig config,
            ExecutorService executor,
            WfbManagedStreamsCallback callback)
            throws WfbLinkException {
        validateManagedStreamsConfig(config);
        return new WfbManagedStreamsSession(this, config, executor, callback);
    }

    static void validateManagedStreamsConfig(WfbManagedStreamsConfig config)
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
        requireRadioPort("uplink_radio_port", config.uplinkRadioPort);
        requireRadioPort("downlink_radio_port", config.downlinkRadioPort);
        requireUdpPort("runtime_bind_port", config.runtimeBindPort);
        requireUdpPort("tx_bind_port", config.txBindPort);
        requireUdpPort("raw_tx_port", config.rawTxPort);
        requireUdpPort("rx_aggregator_port", config.rxAggregatorPort);
        requireUdpPort("raw_rx_port", config.rawRxPort);
        requireDistinctUdpPorts(config);
        requirePositive("raw_payload_bytes", config.rawPayloadBytes);
        requirePositive("tx_payload_interval_ms", config.txPayloadIntervalMs);
        requirePositive("tx_bandwidth_mhz", config.txBandwidthMhz);
        requireNonNegative("tx_mcs", config.txMcs);
        requirePositive("tx_fec_k", config.txFecK);
        requirePositive("tx_fec_n", config.txFecN);
        if (config.txFecK > config.txFecN) {
            throw new WfbLinkException(
                    "android_sdk_invalid_tx_fec", "tx FEC k must be less than or equal to n");
        }
        validateNamedStreams(config);
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

    private static void requireNonNegative(String field, int value) throws WfbLinkException {
        if (value < 0) {
            throw new WfbLinkException(
                    "android_sdk_invalid_" + field, field + " must be non-negative");
        }
    }

    private static void requireRadioPort(String field, int value) throws WfbLinkException {
        if (value <= 0 || value > 255) {
            throw new WfbLinkException(
                    "android_sdk_invalid_" + field, field + " must be in 1..255");
        }
    }

    private static void requireUdpPort(String field, int value) throws WfbLinkException {
        if (value <= 0 || value > 65535) {
            throw new WfbLinkException(
                    "android_sdk_invalid_" + field, field + " must be in 1..65535");
        }
    }

    private static void requireDistinctUdpPorts(WfbManagedStreamsConfig config)
            throws WfbLinkException {
        Set<Integer> ports = new HashSet<Integer>();
        requireDistinctUdpPort(ports, "runtime_bind_port", config.runtimeBindPort);
        requireDistinctUdpPort(ports, "tx_bind_port", config.txBindPort);
        requireDistinctUdpPort(ports, "raw_tx_port", config.rawTxPort);
        requireDistinctUdpPort(ports, "rx_aggregator_port", config.rxAggregatorPort);
        requireDistinctUdpPort(ports, "raw_rx_port", config.rawRxPort);
    }

    private static void requireDistinctUdpPort(Set<Integer> ports, String field, int value)
            throws WfbLinkException {
        if (!ports.add(Integer.valueOf(value))) {
            throw new WfbLinkException(
                    "android_sdk_duplicate_udp_port", "duplicate UDP port for " + field);
        }
    }

    private static void validateNamedStreams(WfbManagedStreamsConfig config)
            throws WfbLinkException {
        if (config.streams == null || config.streams.isEmpty()) {
            return;
        }

        Set<String> names = new HashSet<String>();
        Set<Integer> localUdpPorts = new HashSet<Integer>();
        WfbManagedStream tx = null;
        WfbManagedStream rx = null;
        for (WfbManagedStream stream : config.streams) {
            if (stream == null) {
                throw new WfbLinkException(
                        "android_sdk_invalid_stream", "stream entries must not be null");
            }
            requireStreamString("stream_name", stream.name);
            if (!names.add(stream.name)) {
                throw new WfbLinkException(
                        "android_sdk_duplicate_stream_name",
                        "duplicate managed stream name: " + stream.name);
            }
            if (stream.direction == null) {
                throw new WfbLinkException(
                        "android_sdk_invalid_stream_direction",
                        "stream direction is required for " + stream.name);
            }
            if (stream.payloadKind != WfbPayloadKind.RAW_APPLICATION_DATAGRAM) {
                throw new WfbLinkException(
                        "android_sdk_unsupported_payload_kind",
                        "Android managed streams currently support raw application datagrams only");
            }
            if (stream.criticality == null) {
                throw new WfbLinkException(
                        "android_sdk_invalid_stream_criticality",
                        "stream criticality is required for " + stream.name);
            }
            requireRadioPort("stream_radio_port", stream.radioPort);
            requireUdpPort("stream_local_udp_port", stream.localUdpPort);
            requirePositive("stream_link_id", stream.linkId);
            if (!localUdpPorts.add(Integer.valueOf(stream.localUdpPort))) {
                throw new WfbLinkException(
                        "android_sdk_duplicate_stream_udp_port",
                        "duplicate managed stream UDP port: " + stream.localUdpPort);
            }
            if (stream.direction == WfbStreamDirection.TX) {
                if (tx != null) {
                    throw new WfbLinkException(
                            "android_sdk_unsupported_stream_shape",
                            "Android managed streams currently support exactly one TX stream");
                }
                tx = stream;
            } else if (stream.direction == WfbStreamDirection.RX) {
                if (rx != null) {
                    throw new WfbLinkException(
                            "android_sdk_unsupported_stream_shape",
                            "Android managed streams currently support exactly one RX stream");
                }
                rx = stream;
            }
        }
        if (tx == null || rx == null) {
            throw new WfbLinkException(
                    "android_sdk_unsupported_stream_shape",
                    "Android managed streams currently require one TX and one RX stream");
        }
        if (tx.linkId != rx.linkId) {
            throw new WfbLinkException(
                    "android_sdk_unsupported_stream_shape",
                    "Android managed streams currently require one shared link ID");
        }
        validateTxProfile(tx.txProfile);
    }

    private static void validateTxProfile(WfbManagedTxProfile profile) throws WfbLinkException {
        if (profile == null) {
            throw new WfbLinkException("android_sdk_tx_profile_missing", "TX profile is required");
        }
        requirePositive("tx_profile_bandwidth_mhz", profile.bandwidthMhz);
        requireNonNegative("tx_profile_mcs", profile.mcs);
        requirePositive("tx_profile_fec_k", profile.fecK);
        requirePositive("tx_profile_fec_n", profile.fecN);
        if (profile.fecK > profile.fecN) {
            throw new WfbLinkException(
                    "android_sdk_invalid_tx_profile_fec",
                    "TX profile FEC k must be less than or equal to n");
        }
    }

    private static void requireStreamString(String field, String value) throws WfbLinkException {
        if (value == null || value.length() == 0) {
            throw new WfbLinkException("android_sdk_" + field + "_missing", field + " is required");
        }
    }
}
