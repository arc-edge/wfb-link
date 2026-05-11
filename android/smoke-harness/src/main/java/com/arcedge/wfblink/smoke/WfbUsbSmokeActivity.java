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
import android.graphics.Color;
import android.graphics.Typeface;
import android.os.Build;
import android.os.Bundle;
import android.util.Log;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowInsets;
import android.widget.ScrollView;
import android.widget.TextView;
import java.io.File;

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
    private static final String EXTRA_CHANNEL_NUMBER = "channelNumber";
    private static final String EXTRA_RUN_MANAGED_STREAMS = "runManagedStreams";
    private static final String EXTRA_MANAGED_DURATION_MS = "managedDurationMs";
    private static final String EXTRA_MANAGED_PAYLOAD_COUNT = "managedPayloadCount";
    private static final int DEFAULT_CHANNEL_NUMBER = 36;
    private static final int DEFAULT_MANAGED_DURATION_MS = 12000;
    private static final int DEFAULT_MANAGED_PAYLOAD_COUNT = 20;
    private static final int RX_READ_BUFFER_LEN = 16 * 1024;
    private static final int TIMEOUT_MS = 500;
    private static final int INIT_RX_TIMEOUT_MS = 5000;
    private static final int STATUS_HORIZONTAL_PADDING_DP = 16;
    private static final int STATUS_TOP_PADDING_DP = 64;
    private static final int STATUS_BOTTOM_PADDING_DP = 16;
    private static final int SMOKE_RX_TIMEOUT = -4;

    private UsbManager usbManager;
    private ScrollView statusScroller;
    private TextView status;
    private UsbDeviceConnection activeConnection;
    private UsbInterface activeInterface;
    private int channelNumber;

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
        requestWindowFeature(Window.FEATURE_NO_TITLE);
        super.onCreate(savedInstanceState);
        channelNumber = smokeChannelNumberFromIntent();
        if (getActionBar() != null) {
            getActionBar().hide();
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            getWindow().setDecorFitsSystemWindows(true);
        }
        statusScroller = new ScrollView(this);
        statusScroller.setFillViewport(true);
        statusScroller.setClipToPadding(false);
        statusScroller.setBackgroundColor(Color.WHITE);
        status = new TextView(this);
        status.setTextColor(Color.rgb(24, 24, 24));
        status.setTextSize(14);
        status.setTypeface(Typeface.MONOSPACE);
        status.setTextIsSelectable(true);
        installStatusInsetsPadding();
        statusScroller.addView(
                status,
                new ScrollView.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.WRAP_CONTENT));
        setContentView(statusScroller);
        statusScroller.post(() -> statusScroller.requestApplyInsets());
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
        log("Smoke channel=" + channelNumber + " HT20");
        logPackagedHelpers();
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
                        channelNumber,
                        RX_READ_BUFFER_LEN,
                        INIT_RX_TIMEOUT_MS);
        if (rxResult >= 0) {
            log("RX read smoke completed: parsed_frames=" + rxResult);
        } else if (rxResult == SMOKE_RX_TIMEOUT) {
            log("RX read smoke idle: no packet before timeout");
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
                        channelNumber,
                        RX_READ_BUFFER_LEN,
                        INIT_RX_TIMEOUT_MS);
        if (initRxResult >= 0) {
            log("Init + RX descriptor smoke completed: parsed_frames=" + initRxResult);
        } else if (initRxResult == SMOKE_RX_TIMEOUT) {
            log("Init + RX descriptor smoke idle: no packet before timeout");
        } else {
            log("Init + RX descriptor smoke failed with code " + initRxResult);
        }

        log("Running init + TX/WFB submit smoke");
        int initTxResult =
                WfbNativeSmoke.runInitTxSmoke(
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
                        channelNumber,
                        INIT_RX_TIMEOUT_MS);
        if (initTxResult >= 0) {
            log("Init + TX/WFB submit smoke completed: submitted_frames=" + initTxResult);
        } else {
            log("Init + TX/WFB submit smoke failed with code " + initTxResult);
        }

        if (getIntent().getBooleanExtra(EXTRA_RUN_MANAGED_STREAMS, false)) {
            int managedDurationMs = managedDurationMsFromIntent();
            int managedPayloadCount = managedPayloadCountFromIntent();
            log(
                    "Running managed-stream smoke duration_ms="
                            + managedDurationMs
                            + " payloads="
                            + managedPayloadCount);
            int managedResult =
                    WfbNativeSmoke.runManagedStreamsSmoke(
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
                            channelNumber,
                            INIT_RX_TIMEOUT_MS,
                            getApplicationInfo().nativeLibraryDir,
                            getFilesDir().getAbsolutePath(),
                            managedDurationMs,
                            managedPayloadCount);
            if (managedResult >= 0) {
                log(
                        "Managed-stream smoke completed: submitted_frames="
                                + managedResult);
            } else {
                log("Managed-stream smoke failed with code " + managedResult);
            }
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

    private int smokeChannelNumberFromIntent() {
        int requested = getIntent().getIntExtra(EXTRA_CHANNEL_NUMBER, DEFAULT_CHANNEL_NUMBER);
        if (requested <= 0 || requested > 196) {
            return DEFAULT_CHANNEL_NUMBER;
        }
        return requested;
    }

    private int managedDurationMsFromIntent() {
        int requested = getIntent().getIntExtra(EXTRA_MANAGED_DURATION_MS, DEFAULT_MANAGED_DURATION_MS);
        if (requested <= 0 || requested > 60000) {
            return DEFAULT_MANAGED_DURATION_MS;
        }
        return requested;
    }

    private int managedPayloadCountFromIntent() {
        int requested =
                getIntent().getIntExtra(EXTRA_MANAGED_PAYLOAD_COUNT, DEFAULT_MANAGED_PAYLOAD_COUNT);
        if (requested < 0 || requested > 1000) {
            return DEFAULT_MANAGED_PAYLOAD_COUNT;
        }
        return requested;
    }

    private void logPackagedHelpers() {
        File nativeDir = new File(getApplicationInfo().nativeLibraryDir);
        log("Native library dir=" + nativeDir.getAbsolutePath());
        logPackagedHelper(nativeDir, "libwfb_tx_exec.so");
        logPackagedHelper(nativeDir, "libwfb_rx_exec.so");
        logPackagedHelper(nativeDir, "libwfb_keygen_exec.so");
    }

    private void logPackagedHelper(File nativeDir, String name) {
        File helper = new File(nativeDir, name);
        log(
                "Packaged helper "
                        + name
                        + ": exists="
                        + helper.exists()
                        + " executable="
                        + helper.canExecute());
    }

    private void log(String line) {
        Log.i(TAG, line);
        status.append(line + "\n");
        statusScroller.post(() -> statusScroller.fullScroll(ScrollView.FOCUS_DOWN));
    }

    private void installStatusInsetsPadding() {
        statusScroller.setPadding(
                dp(STATUS_HORIZONTAL_PADDING_DP),
                dp(STATUS_TOP_PADDING_DP),
                dp(STATUS_HORIZONTAL_PADDING_DP),
                dp(STATUS_BOTTOM_PADDING_DP));
        statusScroller.setOnApplyWindowInsetsListener(
                (view, insets) -> {
                    int topInset = systemBarTopInset(insets);
                    int bottomInset = systemBarBottomInset(insets);
                    view.setPadding(
                            dp(STATUS_HORIZONTAL_PADDING_DP),
                            topInset + dp(STATUS_TOP_PADDING_DP),
                            dp(STATUS_HORIZONTAL_PADDING_DP),
                            bottomInset + dp(STATUS_BOTTOM_PADDING_DP));
                    return insets;
                });
    }

    private int systemBarTopInset(WindowInsets insets) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            return insets.getInsets(WindowInsets.Type.systemBars()).top;
        }
        return insets.getSystemWindowInsetTop();
    }

    private int systemBarBottomInset(WindowInsets insets) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            return insets.getInsets(WindowInsets.Type.systemBars()).bottom;
        }
        return insets.getSystemWindowInsetBottom();
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }
}
