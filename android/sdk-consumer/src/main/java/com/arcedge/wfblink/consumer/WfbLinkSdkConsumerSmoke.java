package com.arcedge.wfblink.consumer;

import android.content.Context;
import com.arcedge.wfblink.sdk.WfbLinkManager;
import com.arcedge.wfblink.sdk.WfbManagedStreamsConfig;
import com.arcedge.wfblink.sdk.WfbUsbHandoff;

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
                .build();
    }

    public static WfbLinkManager createManager() {
        return new WfbLinkManager();
    }
}
