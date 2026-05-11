package com.arcedge.wfblink.sdk;

/** Product-facing health summary for a completed managed WFB stream session. */
public final class WfbManagedStreamsHealth {
    public final boolean ok;
    public final String code;
    public final String message;
    public final String runtimeResult;
    public final String stopReason;
    public final WfbManagedHelperStatus helperStatus;
    public final WfbManagedDirectionStats uplink;
    public final WfbManagedDirectionStats downlink;
    public final WfbRuntimeSignalSummary rxSignal;

    WfbManagedStreamsHealth(
            boolean ok,
            String code,
            String message,
            String runtimeResult,
            String stopReason,
            WfbManagedHelperStatus helperStatus,
            WfbManagedDirectionStats uplink,
            WfbManagedDirectionStats downlink,
            WfbRuntimeSignalSummary rxSignal) {
        this.ok = ok;
        this.code = code;
        this.message = message;
        this.runtimeResult = runtimeResult;
        this.stopReason = stopReason;
        this.helperStatus = helperStatus;
        this.uplink = uplink;
        this.downlink = downlink;
        this.rxSignal = rxSignal;
    }

    public boolean reachedRuntimeStop() {
        return "duration_elapsed".equals(stopReason) || "signal".equals(stopReason);
    }

    public boolean hasTxDrops() {
        return uplink.hasLossOrDrops();
    }

    public boolean hasRxSignal() {
        return rxSignal.hasRssi() || rxSignal.hasSnr();
    }

    public boolean isProductionHealthy() {
        return ok
                && reachedRuntimeStop()
                && !hasTxDrops()
                && helperStatus.helpersExitedCleanly();
    }

    public boolean isDegraded() {
        return ok && !isProductionHealthy();
    }

    public String summaryLabel() {
        if (!ok) {
            return code == null || code.length() == 0 ? "failed" : code;
        }
        return isProductionHealthy() ? "healthy" : "degraded";
    }
}
