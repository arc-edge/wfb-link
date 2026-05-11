package com.arcedge.wfblink.gradlesample;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.hardware.usb.UsbConstants;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbEndpoint;
import android.hardware.usb.UsbInterface;
import android.hardware.usb.UsbManager;
import android.os.Binder;
import android.os.Build;
import android.os.IBinder;
import android.util.Log;
import com.arcedge.wfblink.sdk.WfbLinkException;
import com.arcedge.wfblink.sdk.WfbManagedStreamsResult;
import com.arcedge.wfblink.sdk.WfbManagedStreamsSession;
import com.arcedge.wfblink.sdk.WfbManagedStreamsStatus;
import java.io.IOException;
import java.net.DatagramSocket;
import java.net.SocketTimeoutException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicBoolean;

/** Product-shaped foreground service for long-running Android WFB Link sessions. */
public class WfbLinkForegroundService extends Service {
    public static final String ACTION_START = "com.arcedge.wfblink.gradlesample.START_WFB_LINK";
    public static final String ACTION_STOP = "com.arcedge.wfblink.gradlesample.STOP_WFB_LINK";
    private static final String TAG = "WfbLinkFgService";
    private static final String CHANNEL_ID = "wfb-link-radio";
    private static final int NOTIFICATION_ID = 46_161;
    private static final int MAX_DOWNLINK_PAYLOAD_BYTES = 64 * 1024;
    private static final int DOWNLINK_SOCKET_TIMEOUT_MS = 500;

    private final LocalBinder binder = new LocalBinder();
    private final AtomicBoolean stopRequested = new AtomicBoolean(false);
    private final ExecutorService appUdpExecutor = Executors.newSingleThreadExecutor();

    private UsbManager usbManager;
    private NotificationManager notificationManager;
    private WfbLinkGradleSampleController controller;
    private WfbManagedStreamsSession session;
    private UsbDeviceConnection connection;
    private UsbInterface claimedInterface;
    private DatagramSocket controlUplinkSocket;
    private DatagramSocket videoDownlinkSocket;

    public static void start(Context context, UsbDevice device) {
        Intent intent = new Intent(context, WfbLinkForegroundService.class)
                .setAction(ACTION_START)
                .putExtra(UsbManager.EXTRA_DEVICE, device);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(intent);
        } else {
            context.startService(intent);
        }
    }

    public static void requestStop(Context context) {
        Intent intent = new Intent(context, WfbLinkForegroundService.class).setAction(ACTION_STOP);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(intent);
        } else {
            context.startService(intent);
        }
    }

    @Override
    public void onCreate() {
        super.onCreate();
        usbManager = (UsbManager) getSystemService(Context.USB_SERVICE);
        notificationManager = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
        createNotificationChannel();
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        startForegroundNotification("starting");
        String action = intent == null ? null : intent.getAction();
        if (ACTION_STOP.equals(action)) {
            stopSession();
            stopSelf(startId);
            return START_NOT_STICKY;
        }
        UsbDevice device = intent == null ? null : intent.getParcelableExtra(UsbManager.EXTRA_DEVICE);
        if (device == null) {
            onServiceError("wfb_link_missing_usb_device", "start intent did not include a UsbDevice");
            stopSelf(startId);
            return START_NOT_STICKY;
        }
        try {
            startSession(device);
        } catch (WfbLinkException error) {
            onServiceError(error.code, error.getMessage());
            stopSession();
            stopSelf(startId);
            return START_NOT_STICKY;
        } catch (IOException error) {
            onServiceError("wfb_link_app_udp_failed", error.getMessage());
            stopSession();
            stopSelf(startId);
            return START_NOT_STICKY;
        }
        return START_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    @Override
    public void onDestroy() {
        stopSession();
        appUdpExecutor.shutdownNow();
        super.onDestroy();
    }

    public WfbManagedStreamsStatus status() {
        return session == null ? null : session.status();
    }

    public void sendControlPayload(byte[] payload) throws IOException {
        DatagramSocket socket = controlUplinkSocket;
        if (socket == null || socket.isClosed()) {
            throw new IOException("control uplink socket is not open");
        }
        controller.sendControlPayload(socket, payload);
    }

    protected void onVideoDownlinkPayload(byte[] payload) {}

    protected void onStatus(WfbManagedStreamsStatus status) {
        updateNotification(status.summaryLabel());
    }

    protected void onResult(WfbManagedStreamsResult result) {
        updateNotification(result.health.summaryLabel());
        finishSession();
        stopSelf();
    }

    protected void onServiceError(String code, String message) {
        Log.e(TAG, code + ": " + message);
        updateNotification(code);
        if (session != null) {
            finishSession();
            stopSelf();
        }
    }

    private void startSession(UsbDevice device) throws WfbLinkException, IOException {
        if (session != null && !session.isTerminal()) {
            throw new WfbLinkException(
                    "wfb_link_service_already_running", "WFB Link foreground service is already running");
        }
        UsbInterface dataInterface = selectDataInterface(device);
        UsbDeviceConnection opened = usbManager.openDevice(device);
        if (opened == null) {
            throw new WfbLinkException(
                    "wfb_link_usb_open_failed", "UsbManager.openDevice returned null");
        }
        connection = opened;
        claimedInterface = dataInterface;
        controller =
                new WfbLinkGradleSampleController(this, usbManager) {
                    @Override
                    protected void onStatus(WfbManagedStreamsStatus status) {
                        WfbLinkForegroundService.this.onStatus(status);
                    }

                    @Override
                    protected void onResult(WfbManagedStreamsResult result) {
                        WfbLinkForegroundService.this.onResult(result);
                    }

                    @Override
                    protected void onError(WfbLinkException error) {
                        WfbLinkForegroundService.this.onServiceError(error.code, error.getMessage());
                    }
                };
        videoDownlinkSocket = controller.openVideoDownlinkSocket();
        controlUplinkSocket = controller.openControlUplinkSocket();
        session = controller.start(device, connection, dataInterface);
        stopRequested.set(false);
        startVideoDownlinkReceiver();
        updateNotification("running");
    }

    private void startVideoDownlinkReceiver() {
        appUdpExecutor.submit(
                new Runnable() {
                    @Override
                    public void run() {
                        while (!stopRequested.get()) {
                            try {
                                byte[] payload =
                                        controller.receiveVideoDownlinkPayload(
                                                videoDownlinkSocket,
                                                MAX_DOWNLINK_PAYLOAD_BYTES,
                                                DOWNLINK_SOCKET_TIMEOUT_MS);
                                onVideoDownlinkPayload(payload);
                            } catch (SocketTimeoutException timeout) {
                                continue;
                            } catch (IOException error) {
                                if (!stopRequested.get()) {
                                    onServiceError("wfb_link_downlink_udp_failed", error.getMessage());
                                }
                                return;
                            }
                        }
                    }
                });
    }

    private void stopSession() {
        finishSession();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            stopForeground(STOP_FOREGROUND_REMOVE);
        } else {
            stopForeground(true);
        }
    }

    private synchronized void finishSession() {
        stopRequested.set(true);
        closeSocket(controlUplinkSocket);
        controlUplinkSocket = null;
        closeSocket(videoDownlinkSocket);
        videoDownlinkSocket = null;
        if (controller != null) {
            controller.shutdown();
            controller = null;
        }
        if (connection != null) {
            if (claimedInterface != null) {
                connection.releaseInterface(claimedInterface);
                claimedInterface = null;
            }
            connection.close();
            connection = null;
        }
        session = null;
    }

    private static void closeSocket(DatagramSocket socket) {
        if (socket != null) {
            socket.close();
        }
    }

    private UsbInterface selectDataInterface(UsbDevice device) throws WfbLinkException {
        for (int index = 0; index < device.getInterfaceCount(); index++) {
            UsbInterface candidate = device.getInterface(index);
            if (hasBulkInAndOut(candidate)) {
                return candidate;
            }
        }
        throw new WfbLinkException(
                "wfb_link_usb_interface_missing", "no RTL8812AU bulk data interface found");
    }

    private static boolean hasBulkInAndOut(UsbInterface usbInterface) {
        boolean bulkIn = false;
        boolean bulkOut = false;
        for (int index = 0; index < usbInterface.getEndpointCount(); index++) {
            UsbEndpoint endpoint = usbInterface.getEndpoint(index);
            if (endpoint.getType() != UsbConstants.USB_ENDPOINT_XFER_BULK) {
                continue;
            }
            if (endpoint.getDirection() == UsbConstants.USB_DIR_IN) {
                bulkIn = true;
            } else if (endpoint.getDirection() == UsbConstants.USB_DIR_OUT) {
                bulkOut = true;
            }
        }
        return bulkIn && bulkOut;
    }

    private void createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return;
        }
        NotificationChannel channel =
                new NotificationChannel(
                        CHANNEL_ID, "WFB Link radio", NotificationManager.IMPORTANCE_LOW);
        notificationManager.createNotificationChannel(channel);
    }

    private void startForegroundNotification(String status) {
        Notification notification = buildNotification(status);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                    NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
    }

    private void updateNotification(String status) {
        if (notificationManager != null) {
            notificationManager.notify(NOTIFICATION_ID, buildNotification(status));
        }
    }

    private Notification buildNotification(String status) {
        Notification.Builder builder =
                Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
                        ? new Notification.Builder(this, CHANNEL_ID)
                        : new Notification.Builder(this);
        builder.setContentTitle("WFB Link radio")
                .setContentText(status)
                .setSmallIcon(android.R.drawable.stat_sys_upload)
                .setOngoing(true);
        Intent launchIntent = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (launchIntent != null) {
            PendingIntent pendingIntent =
                    PendingIntent.getActivity(
                            this,
                            0,
                            launchIntent,
                            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            builder.setContentIntent(pendingIntent);
        }
        return builder.build();
    }

    public final class LocalBinder extends Binder {
        public WfbLinkForegroundService service() {
            return WfbLinkForegroundService.this;
        }
    }
}
