package com.arcedge.wfblink.smoke;

final class WfbNativeSmoke {
    static {
        System.loadLibrary("wfb_android_smoke");
    }

    private WfbNativeSmoke() {}

    static native int runRegisterSmoke(
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
