package com.arcedge.wfblink.consumer;

import android.content.Context;
import com.arcedge.wfblink.sdk.WfbLinkManager;
import com.arcedge.wfblink.sdk.WfbManagedStream;
import com.arcedge.wfblink.sdk.WfbManagedStreamsConfig;
import com.arcedge.wfblink.sdk.WfbManagedStreamsSession;
import com.arcedge.wfblink.sdk.WfbManagedTxProfile;
import com.arcedge.wfblink.sdk.WfbUsbHandoff;
import com.arcedge.wfblink.sdk.WfbLinkException;
import java.util.concurrent.ExecutorService;

public final class WfbLinkSdkConsumerSmoke {
    private WfbLinkSdkConsumerSmoke() {}

    public static WfbManagedStreamsConfig createConfig(Context context, WfbUsbHandoff usb) {
        return WfbManagedStreamsConfig.builder(context, usb)
                .keyPath(context.getFilesDir().getAbsolutePath() + "/gs.key")
                .initAssets(
                        context.getFilesDir().getAbsolutePath() + "/rtl8812aefw.bin",
                        context.getFilesDir().getAbsolutePath() + "/halhwimg8812a_mac.c",
                        context.getFilesDir().getAbsolutePath() + "/halhwimg8812a_bb.c",
                        context.getFilesDir().getAbsolutePath() + "/halhwimg8812a_rf.c")
                .channelNumber(161)
                .durationMs(15000)
                .payloadCount(20)
                .addStream(
                        WfbManagedStream.tx("control-up", 6, 15606)
                                .txProfile(WfbManagedTxProfile.of(20, 0, 2, 4))
                                .build())
                .addStream(WfbManagedStream.rx("video-down", 4, 15904).build())
                .build();
    }

    public static WfbLinkManager createManager() {
        return new WfbLinkManager();
    }

    public static WfbManagedStreamsSession startSession(
            Context context, WfbUsbHandoff usb, ExecutorService executor) throws WfbLinkException {
        return createManager().startManagedStreams(createConfig(context, usb), executor);
    }
}
