package com.arcedge.wfblink.sdk;

/** Status labels for SDK-supervised WFB-NG helper processes. */
public final class WfbManagedHelperStatus {
    public final String txHelperStatus;
    public final String rxHelperStatus;

    WfbManagedHelperStatus(String txHelperStatus, String rxHelperStatus) {
        this.txHelperStatus = txHelperStatus;
        this.rxHelperStatus = rxHelperStatus;
    }

    public boolean helpersExitedCleanly() {
        return isClean(txHelperStatus) && isClean(rxHelperStatus);
    }

    private static boolean isClean(String value) {
        return "running_at_runtime_end".equals(value)
                || "running".equals(value)
                || "exited:0".equals(value)
                || "exited: exit status: 0".equals(value)
                || "unknown".equals(value);
    }
}
