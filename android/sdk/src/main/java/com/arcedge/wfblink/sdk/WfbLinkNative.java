package com.arcedge.wfblink.sdk;

import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbEndpoint;

final class WfbLinkNative {
    private WfbLinkNative() {}

    static native String runManagedStreams(
            UsbDeviceConnection connection,
            UsbEndpoint bulkInEndpointObject,
            UsbEndpoint bulkOutEndpointObject,
            int fd,
            int vid,
            int pid,
            int interfaceNumber,
            int bulkInEndpoint,
            int bulkOutEndpoint,
            int bulkOutEndpointCount,
            int channelNumber,
            int timeoutMs,
            String nativeLibraryDir,
            String workingDir,
            String keyPath,
            String firmwarePath,
            String macTablePath,
            String bbTablePath,
            String rfTablePath,
            int durationMs,
            int payloadCount,
            int linkId,
            int uplinkRadioPort,
            int downlinkRadioPort,
            int runtimeBindPort,
            int txBindPort,
            int rawTxPort,
            int rxAggregatorPort,
            int rawRxPort,
            int rawPayloadBytes,
            int txPayloadIntervalMs,
            int txBandwidthMhz,
            int txMcs,
            int txFecK,
            int txFecN);
}
