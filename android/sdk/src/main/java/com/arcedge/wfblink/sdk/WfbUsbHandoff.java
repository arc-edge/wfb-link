package com.arcedge.wfblink.sdk;

import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbEndpoint;

/** App-owned Android USBHost objects and endpoint metadata for an RTL8812AU adapter. */
public final class WfbUsbHandoff {
    public final UsbDeviceConnection connection;
    public final UsbEndpoint bulkInEndpointObject;
    public final UsbEndpoint bulkOutEndpointObject;
    public final int fd;
    public final int vid;
    public final int pid;
    public final int interfaceNumber;
    public final int bulkInEndpoint;
    public final int bulkOutEndpoint;
    public final int bulkOutEndpointCount;

    public WfbUsbHandoff(
            UsbDeviceConnection connection,
            UsbEndpoint bulkInEndpointObject,
            UsbEndpoint bulkOutEndpointObject,
            int fd,
            int vid,
            int pid,
            int interfaceNumber,
            int bulkInEndpoint,
            int bulkOutEndpoint,
            int bulkOutEndpointCount) {
        this.connection = connection;
        this.bulkInEndpointObject = bulkInEndpointObject;
        this.bulkOutEndpointObject = bulkOutEndpointObject;
        this.fd = fd;
        this.vid = vid;
        this.pid = pid;
        this.interfaceNumber = interfaceNumber;
        this.bulkInEndpoint = bulkInEndpoint;
        this.bulkOutEndpoint = bulkOutEndpoint;
        this.bulkOutEndpointCount = bulkOutEndpointCount;
    }
}
