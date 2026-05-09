package com.arcedge.wfblink.smoke;

final class WfbNativeSmoke {
    static {
        System.loadLibrary("wfb_android_smoke");
    }

    private WfbNativeSmoke() {}

    static native int runRegisterSmoke(
            android.hardware.usb.UsbDeviceConnection connection,
            android.hardware.usb.UsbEndpoint bulkInEndpointObject,
            android.hardware.usb.UsbEndpoint bulkOutEndpointObject,
            int fd,
            int vid,
            int pid,
            int interfaceNumber,
            int bulkInEndpoint,
            int bulkOutEndpoint,
            int bulkOutEndpointCount,
            int registerAddress,
            int timeoutMs);

    static native int runRxReadSmoke(
            android.hardware.usb.UsbDeviceConnection connection,
            android.hardware.usb.UsbEndpoint bulkInEndpointObject,
            android.hardware.usb.UsbEndpoint bulkOutEndpointObject,
            int fd,
            int vid,
            int pid,
            int interfaceNumber,
            int bulkInEndpoint,
            int bulkOutEndpoint,
            int bulkOutEndpointCount,
            int channelNumber,
            int readBufferLen,
            int timeoutMs);

    static native int runInitRxReadSmoke(
            android.hardware.usb.UsbDeviceConnection connection,
            android.hardware.usb.UsbEndpoint bulkInEndpointObject,
            android.hardware.usb.UsbEndpoint bulkOutEndpointObject,
            int fd,
            int vid,
            int pid,
            int interfaceNumber,
            int bulkInEndpoint,
            int bulkOutEndpoint,
            int bulkOutEndpointCount,
            int channelNumber,
            int readBufferLen,
            int timeoutMs);
}
