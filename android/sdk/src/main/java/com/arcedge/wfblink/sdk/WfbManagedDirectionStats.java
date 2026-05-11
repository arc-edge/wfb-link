package com.arcedge.wfblink.sdk;

/** Per-direction counters for one managed WFB session. */
public final class WfbManagedDirectionStats {
    public final WfbStreamDirection direction;
    /** SDK validation payload count. Product apps should count app-owned UDP separately. */
    public final long rawPackets;
    /** SDK validation payload bytes. Product apps should count app-owned UDP separately. */
    public final long rawBytes;
    public final long radioDatagrams;
    public final long submittedFrames;
    public final long failedSubmissions;
    public final long droppedDatagrams;
    public final long parsedFrames;
    public final long forwardedPayloads;
    public final long radioBytesWritten;

    WfbManagedDirectionStats(
            WfbStreamDirection direction,
            long rawPackets,
            long rawBytes,
            long radioDatagrams,
            long submittedFrames,
            long failedSubmissions,
            long droppedDatagrams,
            long parsedFrames,
            long forwardedPayloads,
            long radioBytesWritten) {
        this.direction = direction;
        this.rawPackets = rawPackets;
        this.rawBytes = rawBytes;
        this.radioDatagrams = radioDatagrams;
        this.submittedFrames = submittedFrames;
        this.failedSubmissions = failedSubmissions;
        this.droppedDatagrams = droppedDatagrams;
        this.parsedFrames = parsedFrames;
        this.forwardedPayloads = forwardedPayloads;
        this.radioBytesWritten = radioBytesWritten;
    }

    public boolean hasLossOrDrops() {
        return failedSubmissions > 0 || droppedDatagrams > 0;
    }
}
