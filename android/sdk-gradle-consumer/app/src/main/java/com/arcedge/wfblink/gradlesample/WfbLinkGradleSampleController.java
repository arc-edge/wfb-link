package com.arcedge.wfblink.gradlesample;

import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.hardware.usb.UsbConstants;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbEndpoint;
import android.hardware.usb.UsbInterface;
import android.hardware.usb.UsbManager;
import com.arcedge.wfblink.sdk.WfbLinkException;
import com.arcedge.wfblink.sdk.WfbLinkManager;
import com.arcedge.wfblink.sdk.WfbManagedStream;
import com.arcedge.wfblink.sdk.WfbManagedStreamsCallback;
import com.arcedge.wfblink.sdk.WfbManagedStreamsConfig;
import com.arcedge.wfblink.sdk.WfbManagedStreamsResult;
import com.arcedge.wfblink.sdk.WfbManagedStreamsSession;
import com.arcedge.wfblink.sdk.WfbManagedStreamsStatus;
import com.arcedge.wfblink.sdk.WfbManagedTxProfile;
import com.arcedge.wfblink.sdk.WfbUsbHandoff;
import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Minimal product-style controller for consuming the local WFB Link SDK AAR. */
public final class WfbLinkGradleSampleController {
    public static final String ACTION_USB_PERMISSION =
            "com.arcedge.wfblink.gradlesample.USB_PERMISSION";
    public static final int CONTROL_UP_LOCAL_PORT = 15606;
    public static final int VIDEO_DOWN_LOCAL_PORT = 15904;

    private final Context context;
    private final UsbManager usbManager;
    private final ExecutorService executor = Executors.newSingleThreadExecutor();
    private final WfbLinkManager manager = new WfbLinkManager();
    private WfbManagedStreamsSession session;

    public WfbLinkGradleSampleController(Context context, UsbManager usbManager) {
        this.context = context.getApplicationContext();
        this.usbManager = usbManager;
    }

    public void requestPermission(UsbDevice device) {
        PendingIntent pendingIntent =
                PendingIntent.getBroadcast(
                        context,
                        0,
                        new Intent(ACTION_USB_PERMISSION).setPackage(context.getPackageName()),
                        PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_MUTABLE);
        usbManager.requestPermission(device, pendingIntent);
    }

    public WfbManagedStreamsSession start(
            UsbDevice device, UsbDeviceConnection connection, UsbInterface dataInterface)
            throws WfbLinkException {
        if (session != null && !session.status().isTerminal()) {
            throw new WfbLinkException(
                    "sample_session_already_running", "WFB Link session is already running");
        }
        if (!connection.claimInterface(dataInterface, true)) {
            throw new WfbLinkException(
                    "sample_usb_claim_failed", "failed to claim RTL8812AU data interface");
        }

        EndpointSelection endpoints = selectBulkEndpoints(dataInterface);
        WfbUsbHandoff usb =
                new WfbUsbHandoff(
                        connection,
                        endpoints.bulkIn,
                        endpoints.bulkOut,
                        connection.getFileDescriptor(),
                        device.getVendorId(),
                        device.getProductId(),
                        dataInterface.getId(),
                        endpoints.bulkIn.getAddress(),
                        endpoints.bulkOut.getAddress(),
                        endpoints.bulkOutCount);

        String files = context.getFilesDir().getAbsolutePath();
        WfbManagedStreamsConfig config =
                WfbManagedStreamsConfig.builder(context, usb)
                        .keyPath(files + "/gs.key")
                        .initAssets(
                                files + "/rtl8812aefw.bin",
                                files + "/halhwimg8812a_mac.c",
                                files + "/halhwimg8812a_bb.c",
                                files + "/halhwimg8812a_rf.c")
                        .channelNumber(161)
                        .durationMs(15000)
                        .payloadCount(20)
                        .addStream(
                                WfbManagedStream.tx("control-up", 6, CONTROL_UP_LOCAL_PORT)
                                        .txProfile(WfbManagedTxProfile.of(20, 0, 2, 4))
                                        .build())
                        .addStream(
                                WfbManagedStream.rx("video-down", 4, VIDEO_DOWN_LOCAL_PORT)
                                        .build())
                        .build();
        session = manager.startManagedStreams(config, executor, new LoggingCallback());
        return session;
    }

    public void sendControlPayload(byte[] payload) throws IOException {
        DatagramSocket socket = new DatagramSocket();
        try {
            DatagramPacket packet =
                    new DatagramPacket(
                            payload,
                            payload.length,
                            InetAddress.getByName("127.0.0.1"),
                            CONTROL_UP_LOCAL_PORT);
            socket.send(packet);
        } finally {
            socket.close();
        }
    }

    public DatagramSocket openVideoDownlinkSocket() throws IOException {
        return new DatagramSocket(VIDEO_DOWN_LOCAL_PORT, InetAddress.getByName("127.0.0.1"));
    }

    public byte[] receiveVideoDownlinkPayload(
            DatagramSocket socket, int maxBytes, int timeoutMs) throws IOException {
        socket.setSoTimeout(timeoutMs);
        byte[] buffer = new byte[maxBytes];
        DatagramPacket packet = new DatagramPacket(buffer, buffer.length);
        socket.receive(packet);
        byte[] payload = new byte[packet.getLength()];
        System.arraycopy(packet.getData(), packet.getOffset(), payload, 0, packet.getLength());
        return payload;
    }

    public boolean requestStop() {
        return session != null && session.requestStop();
    }

    public void shutdown() {
        if (session != null) {
            session.requestStop();
        }
        executor.shutdown();
    }

    protected void onStatus(WfbManagedStreamsStatus status) {}

    protected void onResult(WfbManagedStreamsResult result) {}

    protected void onError(WfbLinkException error) {}

    private EndpointSelection selectBulkEndpoints(UsbInterface dataInterface)
            throws WfbLinkException {
        UsbEndpoint bulkIn = null;
        UsbEndpoint bulkOut = null;
        int bulkOutCount = 0;
        for (int index = 0; index < dataInterface.getEndpointCount(); index++) {
            UsbEndpoint endpoint = dataInterface.getEndpoint(index);
            if (endpoint.getType() != UsbConstants.USB_ENDPOINT_XFER_BULK) {
                continue;
            }
            if (endpoint.getDirection() == UsbConstants.USB_DIR_IN && bulkIn == null) {
                bulkIn = endpoint;
            } else if (endpoint.getDirection() == UsbConstants.USB_DIR_OUT) {
                bulkOutCount++;
                if (bulkOut == null) {
                    bulkOut = endpoint;
                }
            }
        }
        if (bulkIn == null || bulkOut == null) {
            throw new WfbLinkException(
                    "sample_usb_endpoints_missing", "bulk IN and OUT endpoints are required");
        }
        return new EndpointSelection(bulkIn, bulkOut, bulkOutCount);
    }

    private final class LoggingCallback implements WfbManagedStreamsCallback {
        @Override
        public void onStatusChanged(WfbManagedStreamsStatus status) {
            onStatus(status);
        }

        @Override
        public void onCompleted(WfbManagedStreamsResult result) {
            onResult(result);
        }

        @Override
        public void onFailed(WfbLinkException error) {
            onError(error);
        }
    }

    private static final class EndpointSelection {
        final UsbEndpoint bulkIn;
        final UsbEndpoint bulkOut;
        final int bulkOutCount;

        EndpointSelection(UsbEndpoint bulkIn, UsbEndpoint bulkOut, int bulkOutCount) {
            this.bulkIn = bulkIn;
            this.bulkOut = bulkOut;
            this.bulkOutCount = bulkOutCount;
        }
    }
}
