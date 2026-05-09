package com.arcedge.wfblink.smoke;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbEndpoint;
import android.hardware.usb.UsbInterface;
import android.hardware.usb.UsbManager;
import android.os.Build;
import android.os.Bundle;
import android.util.Log;
import android.widget.TextView;

public final class WfbUsbSmokeActivity extends Activity {
    private static final String TAG = "WfbUsbSmoke";
    private static final String ACTION_USB_PERMISSION =
            "com.arcedge.wfblink.smoke.USB_PERMISSION";
    private static final int RTL8812AU_VID = 0x0bda;
    private static final int RTL8812AU_PID = 0x8812;
    private static final int INTERFACE_NUMBER = 0;
    private static final int BULK_IN_ENDPOINT = 0x81;
    private static final int BULK_OUT_ENDPOINT = 0x02;
    private static final int BULK_OUT_ENDPOINT_COUNT = 3;
    private static final int REG_SYS_FUNC_EN = 0x0002;
    private static final int RTL_USB_REQ = 0x05;
    private static final int RTL_READ_REQUEST_TYPE = 0xc0;
    private static final int CHANNEL_NUMBER = 36;
    private static final int RX_READ_BUFFER_LEN = 16 * 1024;
    private static final int TIMEOUT_MS = 500;
    private static final int INIT_RX_TIMEOUT_MS = 5000;

    private UsbManager usbManager;
    private TextView status;
    private UsbDeviceConnection activeConnection;
    private UsbInterface activeInterface;

    private final BroadcastReceiver permissionReceiver =
            new BroadcastReceiver() {
                @Override
                public void onReceive(Context context, Intent intent) {
                    if (!ACTION_USB_PERMISSION.equals(intent.getAction())) {
                        return;
                    }
                    UsbDevice device = intent.getParcelableExtra(UsbManager.EXTRA_DEVICE);
                    boolean granted =
                            intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false);
                    if (device == null || !granted) {
                        log("USB permission denied");
                        return;
                    }
                    runSmoke(device);
                }
            };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        status = new TextView(this);
        status.setTextIsSelectable(true);
        setContentView(status);
        usbManager = (UsbManager) getSystemService(Context.USB_SERVICE);
        IntentFilter permissionFilter = new IntentFilter(ACTION_USB_PERMISSION);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            registerReceiver(permissionReceiver, permissionFilter, Context.RECEIVER_NOT_EXPORTED);
        } else {
            registerReceiver(permissionReceiver, permissionFilter);
        }
        requestFirstMatchingDevice();
    }

    @Override
    protected void onDestroy() {
        unregisterReceiver(permissionReceiver);
        if (activeConnection != null) {
            if (activeInterface != null) {
                activeConnection.releaseInterface(activeInterface);
                activeInterface = null;
            }
            activeConnection.close();
            activeConnection = null;
        }
        super.onDestroy();
    }

    private void requestFirstMatchingDevice() {
        for (UsbDevice device : usbManager.getDeviceList().values()) {
            if (device.getVendorId() == RTL8812AU_VID && device.getProductId() == RTL8812AU_PID) {
                if (usbManager.hasPermission(device)) {
                    log("Found RTL8812AU USB device with existing permission");
                    runSmoke(device);
                    return;
                }
                log("Found RTL8812AU USB device; requesting permission");
                Intent permissionIntent = new Intent(ACTION_USB_PERMISSION).setPackage(getPackageName());
                int flags = PendingIntent.FLAG_UPDATE_CURRENT;
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    flags |= PendingIntent.FLAG_MUTABLE;
                }
                PendingIntent intent =
                        PendingIntent.getBroadcast(this, 0, permissionIntent, flags);
                usbManager.requestPermission(device, intent);
                return;
            }
        }
        log("No RTL8812AU USB device found");
    }

    private void runSmoke(UsbDevice device) {
        activeConnection = usbManager.openDevice(device);
        if (activeConnection == null) {
            log("openDevice returned null");
            return;
        }
        if (device.getInterfaceCount() <= INTERFACE_NUMBER) {
            log("USB device has no interface " + INTERFACE_NUMBER);
            return;
        }
        UsbInterface usbInterface = device.getInterface(INTERFACE_NUMBER);
        if (!activeConnection.claimInterface(usbInterface, true)) {
            log("claimInterface failed for interface " + INTERFACE_NUMBER);
            return;
        }
        activeInterface = usbInterface;
        UsbEndpoint bulkInEndpointObject = findEndpoint(usbInterface, BULK_IN_ENDPOINT);
        UsbEndpoint bulkOutEndpointObject = findEndpoint(usbInterface, BULK_OUT_ENDPOINT);
        if (bulkInEndpointObject == null || bulkOutEndpointObject == null) {
            log("Required bulk endpoint objects not found");
            return;
        }
        int fd = activeConnection.getFileDescriptor();
        byte[] javaRegister = new byte[1];
        int javaRegisterRead =
                activeConnection.controlTransfer(
                        RTL_READ_REQUEST_TYPE,
                        RTL_USB_REQ,
                        REG_SYS_FUNC_EN,
                        0,
                        javaRegister,
                        javaRegister.length,
                        TIMEOUT_MS);
        if (javaRegisterRead == javaRegister.length) {
            log(
                    "Java controlTransfer passed: REG_SYS_FUNC_EN=0x"
                            + Integer.toHexString(javaRegister[0] & 0xff));
        } else {
            log("Java controlTransfer failed: result=" + javaRegisterRead);
        }
        log("Opened device fd=" + fd + "; running register smoke");
        int result =
                WfbNativeSmoke.runRegisterSmoke(
                        activeConnection,
                        bulkInEndpointObject,
                        bulkOutEndpointObject,
                        fd,
                        device.getVendorId(),
                        device.getProductId(),
                        INTERFACE_NUMBER,
                        BULK_IN_ENDPOINT,
                        BULK_OUT_ENDPOINT,
                        BULK_OUT_ENDPOINT_COUNT,
                        REG_SYS_FUNC_EN,
                        TIMEOUT_MS);
        if (result >= 0) {
            log("Register smoke passed: REG_SYS_FUNC_EN=0x" + Integer.toHexString(result));
        } else {
            log("Register smoke failed with code " + result);
            return;
        }

        int rxResult =
                WfbNativeSmoke.runRxReadSmoke(
                        activeConnection,
                        bulkInEndpointObject,
                        bulkOutEndpointObject,
                        fd,
                        device.getVendorId(),
                        device.getProductId(),
                        INTERFACE_NUMBER,
                        BULK_IN_ENDPOINT,
                        BULK_OUT_ENDPOINT,
                        BULK_OUT_ENDPOINT_COUNT,
                        CHANNEL_NUMBER,
                        RX_READ_BUFFER_LEN,
                        INIT_RX_TIMEOUT_MS);
        if (rxResult >= 0) {
            log("RX read smoke completed: parsed_frames=" + rxResult);
        } else {
            log("RX read smoke failed with code " + rxResult);
        }

        log("Running init + RX descriptor smoke");
        int initRxResult =
                WfbNativeSmoke.runInitRxReadSmoke(
                        activeConnection,
                        bulkInEndpointObject,
                        bulkOutEndpointObject,
                        fd,
                        device.getVendorId(),
                        device.getProductId(),
                        INTERFACE_NUMBER,
                        BULK_IN_ENDPOINT,
                        BULK_OUT_ENDPOINT,
                        BULK_OUT_ENDPOINT_COUNT,
                        CHANNEL_NUMBER,
                        RX_READ_BUFFER_LEN,
                        TIMEOUT_MS);
        if (initRxResult >= 0) {
            log("Init + RX descriptor smoke completed: parsed_frames=" + initRxResult);
        } else {
            log("Init + RX descriptor smoke failed with code " + initRxResult);
        }
    }

    private UsbEndpoint findEndpoint(UsbInterface usbInterface, int address) {
        for (int i = 0; i < usbInterface.getEndpointCount(); i++) {
            UsbEndpoint endpoint = usbInterface.getEndpoint(i);
            if (endpoint.getAddress() == address) {
                return endpoint;
            }
        }
        return null;
    }

    private void log(String line) {
        Log.i(TAG, line);
        status.append(line + "\n");
    }
}
