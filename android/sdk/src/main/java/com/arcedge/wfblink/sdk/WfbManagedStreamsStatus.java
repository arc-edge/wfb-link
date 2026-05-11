package com.arcedge.wfblink.sdk;

/** Immutable status snapshot for an Android managed WFB stream session. */
public final class WfbManagedStreamsStatus {
    public final WfbManagedStreamsState state;
    public final boolean stopRequested;
    public final long startedAtMillis;
    public final long finishedAtMillis;
    public final WfbManagedStreamsResult result;
    public final WfbLinkException error;

    WfbManagedStreamsStatus(
            WfbManagedStreamsState state,
            boolean stopRequested,
            long startedAtMillis,
            long finishedAtMillis,
            WfbManagedStreamsResult result,
            WfbLinkException error) {
        this.state = state;
        this.stopRequested = stopRequested;
        this.startedAtMillis = startedAtMillis;
        this.finishedAtMillis = finishedAtMillis;
        this.result = result;
        this.error = error;
    }

    static WfbManagedStreamsStatus created() {
        return new WfbManagedStreamsStatus(
                WfbManagedStreamsState.CREATED, false, 0, 0, null, null);
    }

    WfbManagedStreamsStatus asRunning(long nowMillis) {
        return new WfbManagedStreamsStatus(
                WfbManagedStreamsState.RUNNING, stopRequested, nowMillis, 0, null, null);
    }

    WfbManagedStreamsStatus withStopRequested() {
        if (isTerminal()) {
            return this;
        }
        return new WfbManagedStreamsStatus(
                WfbManagedStreamsState.STOP_REQUESTED,
                true,
                startedAtMillis,
                finishedAtMillis,
                result,
                error);
    }

    WfbManagedStreamsStatus withResult(WfbManagedStreamsResult value, long nowMillis) {
        return new WfbManagedStreamsStatus(
                WfbManagedStreamsState.SUCCEEDED,
                stopRequested,
                startedAtMillis,
                nowMillis,
                value,
                null);
    }

    WfbManagedStreamsStatus withError(WfbLinkException value, long nowMillis) {
        return new WfbManagedStreamsStatus(
                WfbManagedStreamsState.FAILED,
                stopRequested,
                startedAtMillis,
                nowMillis,
                null,
                value);
    }

    public boolean isTerminal() {
        return state == WfbManagedStreamsState.SUCCEEDED || state == WfbManagedStreamsState.FAILED;
    }
}
