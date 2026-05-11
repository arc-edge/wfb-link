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
import android.view.WindowManager;
import android.widget.ScrollView;
import android.widget.TextView;
import com.arcedge.wfblink.sdk.WfbLinkException;
import com.arcedge.wfblink.sdk.WfbLinkManager;
import com.arcedge.wfblink.sdk.WfbManagedStreamsConfig;
import com.arcedge.wfblink.sdk.WfbManagedStreamsResult;
import com.arcedge.wfblink.sdk.WfbManagedStreamsSession;
import com.arcedge.wfblink.sdk.WfbUsbHandoff;
import java.io.File;
import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.nio.ByteBuffer;
import java.nio.channels.DatagramChannel;
import java.util.concurrent.Callable;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.atomic.AtomicBoolean;

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
    private static final String EXTRA_MANAGED_ONLY = "managedOnly";
    private static final String EXTRA_MANAGED_VALIDATION_TRAFFIC = "managedValidationTraffic";
    private static final String EXTRA_MANAGED_DURATION_MS = "managedDurationMs";
    private static final String EXTRA_MANAGED_PAYLOAD_COUNT = "managedPayloadCount";
    private static final String EXTRA_MANAGED_PAYLOAD_INTERVAL_MS = "managedPayloadIntervalMs";
    private static final int DEFAULT_CHANNEL_NUMBER = 36;
    private static final int DEFAULT_MANAGED_DURATION_MS = 12000;
    private static final int DEFAULT_MANAGED_PAYLOAD_COUNT = 20;
    private static final int DEFAULT_MANAGED_PAYLOAD_INTERVAL_MS = 20;
    private static final int MAX_MANAGED_DURATION_MS = 3600000;
    private static final int MAX_MANAGED_PAYLOAD_COUNT = 1000000;
    private static final int MAX_MANAGED_PAYLOAD_INTERVAL_MS = 60000;
    private static final String SMOKE_ASSET_DIR = "/data/local/tmp/wfb-link";
    private static final String SMOKE_KEY_PATH = SMOKE_ASSET_DIR + "/gs.key";
    private static final String SMOKE_FIRMWARE_PATH = SMOKE_ASSET_DIR + "/rtl8812aefw.bin";
    private static final String SMOKE_MAC_TABLE_PATH = SMOKE_ASSET_DIR + "/halhwimg8812a_mac.c";
    private static final String SMOKE_BB_TABLE_PATH = SMOKE_ASSET_DIR + "/halhwimg8812a_bb.c";
    private static final String SMOKE_RF_TABLE_PATH = SMOKE_ASSET_DIR + "/halhwimg8812a_rf.c";
    private static final int RX_READ_BUFFER_LEN = 16 * 1024;
    private static final int TIMEOUT_MS = 500;
    private static final int INIT_RX_TIMEOUT_MS = 5000;
    private static final int MANAGED_RAW_TX_PORT = 15606;
    private static final int MANAGED_RAW_RX_PORT = 15904;
    private static final int MANAGED_RAW_PAYLOAD_BYTES = 512;
    private static final int PRODUCT_MODE_READY_TIMEOUT_MS = 30000;
    private static final int PRODUCT_MODE_RX_GRACE_MS = 5000;
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
    private boolean smokeStarted;

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
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
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
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        if (smokeStarted) {
            log("Ignoring USB attach intent after smoke start");
            return;
        }
        channelNumber = smokeChannelNumberFromIntent();
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
        if (smokeStarted) {
            log("Ignoring duplicate smoke start");
            return;
        }
        smokeStarted = true;
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
        boolean managedOnly = getIntent().getBooleanExtra(EXTRA_MANAGED_ONLY, false);
        if (managedOnly) {
            log("Opened device fd=" + fd + "; skipping diagnostic smokes for managed-only run");
        } else {
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
        }

        if (getIntent().getBooleanExtra(EXTRA_RUN_MANAGED_STREAMS, false)) {
            int managedDurationMs = managedDurationMsFromIntent();
            int managedPayloadCount = managedPayloadCountFromIntent();
            int managedPayloadIntervalMs = managedPayloadIntervalMsFromIntent();
            boolean validationTraffic =
                    getIntent().getBooleanExtra(EXTRA_MANAGED_VALIDATION_TRAFFIC, true);
            log(
                    "Running managed-stream smoke duration_ms="
                            + managedDurationMs
                            + " payloads="
                            + managedPayloadCount
                            + " tx_payload_interval_ms="
                            + managedPayloadIntervalMs
                            + " validation_traffic="
                            + validationTraffic);
            WfbUsbHandoff usb =
                    new WfbUsbHandoff(
                            activeConnection,
                            bulkInEndpointObject,
                            bulkOutEndpointObject,
                            fd,
                            device.getVendorId(),
                            device.getProductId(),
                            INTERFACE_NUMBER,
                            BULK_IN_ENDPOINT,
                            BULK_OUT_ENDPOINT,
                            BULK_OUT_ENDPOINT_COUNT);
            WfbManagedStreamsConfig config =
                    WfbManagedStreamsConfig.builder(this, usb)
                            .keyPath(SMOKE_KEY_PATH)
                            .initAssets(
                                    SMOKE_FIRMWARE_PATH,
                                    SMOKE_MAC_TABLE_PATH,
                                    SMOKE_BB_TABLE_PATH,
                                    SMOKE_RF_TABLE_PATH)
                            .channelNumber(channelNumber)
                            .timeoutMs(INIT_RX_TIMEOUT_MS)
                            .durationMs(managedDurationMs)
                            .payloadCount(managedPayloadCount)
                            .txPayloadIntervalMs(managedPayloadIntervalMs)
                            .validationTrafficEnabled(validationTraffic)
                            .build();
            try {
                if (validationTraffic) {
                    logManagedResult(
                            "Managed-stream smoke",
                            new WfbLinkManager().runManagedStreamsBlocking(config),
                            null,
                            null);
                } else {
                    runManagedProductModeSmoke(
                            config, managedDurationMs, managedPayloadCount, managedPayloadIntervalMs);
                }
            } catch (WfbLinkException error) {
                log(
                        "Managed-stream smoke SDK error code="
                                + error.code
                                + " message="
                                + error.getMessage());
            }
        }
    }

    private void runManagedProductModeSmoke(
            WfbManagedStreamsConfig config, int durationMs, int payloadCount, int payloadIntervalMs)
            throws WfbLinkException {
        File readyFile = new File(getFilesDir(), "android-managed-ready.json");
        if (readyFile.exists() && !readyFile.delete()) {
            log("Product-mode smoke warning: failed to delete stale ready file");
        }
        runProductModeUdpPreflight();
        AtomicBoolean stop = new AtomicBoolean(false);
        ExecutorService executor = Executors.newFixedThreadPool(3);
        Future<AppTrafficSummary> receiver =
                executor.submit(
                        new AppRawReceiver(
                                stop, durationMs + PRODUCT_MODE_RX_GRACE_MS, MANAGED_RAW_RX_PORT));
        WfbManagedStreamsSession session =
                new WfbLinkManager().startManagedStreams(config, executor);
        Future<AppTrafficSummary> sender =
                executor.submit(
                        new AppRawSender(
                                stop,
                                readyFile,
                                payloadCount,
                                payloadIntervalMs,
                                MANAGED_RAW_TX_PORT,
                                MANAGED_RAW_PAYLOAD_BYTES));
        try {
            WfbManagedStreamsResult result = session.await();
            stop.set(true);
            AppTrafficSummary appTx = getAppTrafficSummary(sender, "app_tx");
            AppTrafficSummary appRx = getAppTrafficSummary(receiver, "app_rx");
            if (result.ok
                    && (appTx.hasFailures() || appRx.hasFailures() || appTx.packets != payloadCount)) {
                logManagedResult(
                        "Managed-stream product-mode",
                        result,
                        appTx,
                        appRx,
                        "failed",
                        " code=android_smoke_app_traffic_failed message="
                                + productModeTrafficFailure(payloadCount, appTx, appRx));
            } else {
                logManagedResult("Managed-stream product-mode", result, appTx, appRx);
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new WfbLinkException(
                    "android_smoke_product_mode_interrupted",
                    "product-mode managed smoke interrupted",
                    error);
        } finally {
            stop.set(true);
            executor.shutdownNow();
        }
    }

    private AppTrafficSummary getAppTrafficSummary(
            Future<AppTrafficSummary> future, String direction) throws WfbLinkException {
        try {
            return future.get();
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            return AppTrafficSummary.failed(
                    0,
                    0,
                    "InterruptedException: product-mode " + direction + " traffic interrupted");
        } catch (ExecutionException error) {
            Throwable cause = error.getCause() == null ? error : error.getCause();
            return AppTrafficSummary.failed(
                    0,
                    0,
                    cause.getClass().getSimpleName() + ": " + cause.getMessage());
        }
    }

    private void logManagedResult(
            String label,
            WfbManagedStreamsResult result,
            AppTrafficSummary appTx,
            AppTrafficSummary appRx) {
        logManagedResult(label, result, appTx, appRx, result.ok ? "completed" : "failed", "");
    }

    private void logManagedResult(
            String label,
            WfbManagedStreamsResult result,
            AppTrafficSummary appTx,
            AppTrafficSummary appRx,
            String statusLabel,
            String statusSuffix) {
        String appTraffic =
                appTx == null || appRx == null
                        ? ""
                        : " app_tx="
                                + appTx.format()
                                + " app_rx="
                                + appRx.format();
        if (result.ok && "completed".equals(statusLabel)) {
            log(
                    label
                            + " "
                            + statusLabel
                            + ": submitted_frames="
                            + result.submittedFrames
                            + " tx_datagrams="
                            + result.txDatagrams
                            + " raw_tx="
                            + result.rawTxPackets
                            + " raw_rx="
                            + result.rawRxPackets
                            + " rx_forwarded="
                            + result.rxForwardedPayloads
                            + appTraffic
                            + " health_ok="
                            + result.health.ok
                            + " tx_drops="
                            + result.health.hasTxDrops()
                            + " stop="
                            + result.stopReason
                            + statusSuffix);
        } else {
            log(
                    label
                            + " "
                            + statusLabel
                            + (statusSuffix.isEmpty()
                                    ? " code=" + result.code + " message=" + result.message
                                    : statusSuffix)
                            + " submitted_frames="
                            + result.submittedFrames
                            + " tx_datagrams="
                            + result.txDatagrams
                            + " raw_tx="
                            + result.rawTxPackets
                            + " raw_rx="
                            + result.rawRxPackets
                            + " rx_forwarded="
                            + result.rxForwardedPayloads
                            + appTraffic
                            + " result="
                            + result.runtimeResult
                            + " stop="
                            + result.stopReason);
        }
    }

    private String productModeTrafficFailure(
            int expectedPayloadCount, AppTrafficSummary appTx, AppTrafficSummary appRx) {
        StringBuilder message = new StringBuilder();
        if (appTx.packets != expectedPayloadCount) {
            message.append("app_tx_packets=");
            message.append(appTx.packets);
            message.append(" expected=");
            message.append(expectedPayloadCount);
        }
        if (appTx.hasFailures()) {
            appendFailure(message, "app_tx_error", appTx.firstError);
        }
        if (appRx.hasFailures()) {
            appendFailure(message, "app_rx_error", appRx.firstError);
        }
        if (message.length() == 0) {
            message.append("unknown app traffic failure");
        }
        return message.toString();
    }

    private static void appendFailure(StringBuilder message, String label, String value) {
        if (message.length() > 0) {
            message.append("; ");
        }
        message.append(label);
        message.append("=");
        message.append(value == null ? "unknown" : value);
    }

    private void runProductModeUdpPreflight() throws WfbLinkException {
        ExecutorService executor = Executors.newSingleThreadExecutor();
        try {
            executor.submit(
                            new Callable<Void>() {
                                @Override
                                public Void call() throws IOException {
                                    runProductModeUdpPreflightBlocking();
                                    return null;
                                }
                            })
                    .get();
            log("Product-mode UDP loopback preflight passed");
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new WfbLinkException(
                    "android_smoke_udp_preflight_failed",
                    "product-mode UDP loopback preflight interrupted",
                    error);
        } catch (ExecutionException error) {
            Throwable cause = error.getCause() == null ? error : error.getCause();
            throw new WfbLinkException(
                    "android_smoke_udp_preflight_failed",
                    "product-mode UDP loopback preflight failed: "
                            + cause.getClass().getSimpleName()
                            + ": "
                            + cause.getMessage(),
                    error);
        } finally {
            executor.shutdownNow();
        }
    }

    private static void runProductModeUdpPreflightBlocking() throws IOException {
        InetAddress loopback = InetAddress.getByName("127.0.0.1");
        byte[] payload = managedPayload(0, 64);
        DatagramSocket receiver = new DatagramSocket(0, loopback);
        receiver.setSoTimeout(1000);
        try {
            LoopbackDatagramSender sender = LoopbackDatagramSender.open(receiver.getLocalPort());
            try {
                sender.send(payload);
            } finally {
                sender.close();
            }
            byte[] buffer = new byte[payload.length];
            DatagramPacket packet = new DatagramPacket(buffer, buffer.length);
            receiver.receive(packet);
            if (packet.getLength() != payload.length) {
                throw new IOException(
                        "preflight packet length "
                                + packet.getLength()
                                + " != "
                                + payload.length);
            }
        } finally {
            receiver.close();
        }
    }

    private static byte[] managedPayload(int sequence, int payloadBytes) {
        byte[] payload = new byte[payloadBytes];
        payload[0] = (byte) ((sequence >>> 24) & 0xff);
        payload[1] = (byte) ((sequence >>> 16) & 0xff);
        payload[2] = (byte) ((sequence >>> 8) & 0xff);
        payload[3] = (byte) (sequence & 0xff);
        for (int index = 4; index < payload.length; index++) {
            payload[index] = (byte) ((sequence + index - 4) & 0xff);
        }
        return payload;
    }

    private static final class AppTrafficSummary {
        final long packets;
        final long bytes;
        final long failures;
        final String firstError;

        AppTrafficSummary(long packets, long bytes) {
            this(packets, bytes, 0, null);
        }

        AppTrafficSummary(long packets, long bytes, long failures, String firstError) {
            this.packets = packets;
            this.bytes = bytes;
            this.failures = failures;
            this.firstError = firstError;
        }

        static AppTrafficSummary failed(long packets, long bytes, String firstError) {
            return new AppTrafficSummary(packets, bytes, 1, firstError);
        }

        boolean hasFailures() {
            return failures > 0;
        }

        String format() {
            String formatted = packets + "/" + bytes;
            if (failures > 0) {
                formatted += " failures=" + failures + " first_error=" + firstError;
            }
            return formatted;
        }
    }

    private static final class LoopbackDatagramSender {
        private final DatagramChannel channel;
        private final InetSocketAddress target;

        private LoopbackDatagramSender(DatagramChannel channel, InetSocketAddress target) {
            this.channel = channel;
            this.target = target;
        }

        static LoopbackDatagramSender open(int targetPort) throws IOException {
            InetAddress loopback = InetAddress.getByName("127.0.0.1");
            DatagramChannel channel = DatagramChannel.open();
            boolean success = false;
            try {
                channel.bind(new InetSocketAddress(loopback, 0));
                LoopbackDatagramSender sender =
                        new LoopbackDatagramSender(channel, new InetSocketAddress(loopback, targetPort));
                success = true;
                return sender;
            } finally {
                if (!success) {
                    channel.close();
                }
            }
        }

        void send(byte[] payload) throws IOException {
            int written = channel.send(ByteBuffer.wrap(payload), target);
            if (written != payload.length) {
                throw new IOException("short UDP send " + written + " != " + payload.length);
            }
        }

        void close() throws IOException {
            channel.close();
        }
    }

    private static final class AppRawSender implements Callable<AppTrafficSummary> {
        private final AtomicBoolean stop;
        private final File readyFile;
        private final int payloadCount;
        private final int payloadIntervalMs;
        private final int targetPort;
        private final int payloadBytes;

        AppRawSender(
                AtomicBoolean stop,
                File readyFile,
                int payloadCount,
                int payloadIntervalMs,
                int targetPort,
                int payloadBytes) {
            this.stop = stop;
            this.readyFile = readyFile;
            this.payloadCount = payloadCount;
            this.payloadIntervalMs = payloadIntervalMs;
            this.targetPort = targetPort;
            this.payloadBytes = payloadBytes;
        }

        @Override
        public AppTrafficSummary call() throws Exception {
            long deadline = System.currentTimeMillis() + PRODUCT_MODE_READY_TIMEOUT_MS;
            while (!stop.get() && System.currentTimeMillis() < deadline && !readyFile.exists()) {
                Thread.sleep(50);
            }
            if (stop.get()) {
                return new AppTrafficSummary(0, 0);
            }
            if (!readyFile.exists()) {
                return AppTrafficSummary.failed(
                        0,
                        0,
                        "ready file did not appear within "
                                + PRODUCT_MODE_READY_TIMEOUT_MS
                                + "ms");
            }
            Thread.sleep(250);
            long packets = 0;
            long bytes = 0;
            LoopbackDatagramSender sender = LoopbackDatagramSender.open(targetPort);
            try {
                for (int sequence = 0; sequence < payloadCount && !stop.get(); sequence++) {
                    byte[] payload = managedPayload(sequence, payloadBytes);
                    try {
                        sender.send(payload);
                    } catch (IOException error) {
                        return AppTrafficSummary.failed(
                                packets,
                                bytes,
                                error.getClass().getSimpleName() + ": " + error.getMessage());
                    }
                    packets++;
                    bytes += payload.length;
                    Thread.sleep(payloadIntervalMs);
                }
            } finally {
                sender.close();
            }
            return new AppTrafficSummary(packets, bytes);
        }
    }

    private static final class AppRawReceiver implements Callable<AppTrafficSummary> {
        private final AtomicBoolean stop;
        private final int durationMs;
        private final int listenPort;

        AppRawReceiver(AtomicBoolean stop, int durationMs, int listenPort) {
            this.stop = stop;
            this.durationMs = durationMs;
            this.listenPort = listenPort;
        }

        @Override
        public AppTrafficSummary call() throws IOException {
            long deadline = System.currentTimeMillis() + durationMs;
            long packets = 0;
            long bytes = 0;
            byte[] buffer = new byte[4096];
            DatagramSocket socket =
                    new DatagramSocket(listenPort, InetAddress.getByName("127.0.0.1"));
            socket.setSoTimeout(200);
            try {
                while (!stop.get() && System.currentTimeMillis() < deadline) {
                    DatagramPacket packet = new DatagramPacket(buffer, buffer.length);
                    try {
                        socket.receive(packet);
                    } catch (SocketTimeoutException timeout) {
                        continue;
                    }
                    packets++;
                    bytes += packet.getLength();
                }
            } finally {
                socket.close();
            }
            return new AppTrafficSummary(packets, bytes);
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
        if (requested <= 0 || requested > MAX_MANAGED_DURATION_MS) {
            return DEFAULT_MANAGED_DURATION_MS;
        }
        return requested;
    }

    private int managedPayloadCountFromIntent() {
        int requested =
                getIntent().getIntExtra(EXTRA_MANAGED_PAYLOAD_COUNT, DEFAULT_MANAGED_PAYLOAD_COUNT);
        if (requested < 0 || requested > MAX_MANAGED_PAYLOAD_COUNT) {
            return DEFAULT_MANAGED_PAYLOAD_COUNT;
        }
        return requested;
    }

    private int managedPayloadIntervalMsFromIntent() {
        int requested =
                getIntent()
                        .getIntExtra(
                                EXTRA_MANAGED_PAYLOAD_INTERVAL_MS,
                                DEFAULT_MANAGED_PAYLOAD_INTERVAL_MS);
        if (requested <= 0 || requested > MAX_MANAGED_PAYLOAD_INTERVAL_MS) {
            return DEFAULT_MANAGED_PAYLOAD_INTERVAL_MS;
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
