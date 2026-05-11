package com.arcedge.wfblink.sdk;

import org.json.JSONObject;

/** RX signal summary parsed from the runtime report when radio metadata exists. */
public final class WfbRuntimeSignalSummary {
    public static final WfbRuntimeSignalSummary EMPTY =
            new WfbRuntimeSignalSummary(
                    WfbRuntimeSignalMetric.EMPTY,
                    WfbRuntimeSignalMetric.EMPTY,
                    WfbRuntimeSignalMetric.EMPTY);

    public final WfbRuntimeSignalMetric rssiDbm;
    public final WfbRuntimeSignalMetric snrDb;
    public final WfbRuntimeSignalMetric noiseDbm;

    WfbRuntimeSignalSummary(
            WfbRuntimeSignalMetric rssiDbm,
            WfbRuntimeSignalMetric snrDb,
            WfbRuntimeSignalMetric noiseDbm) {
        this.rssiDbm = rssiDbm;
        this.snrDb = snrDb;
        this.noiseDbm = noiseDbm;
    }

    static WfbRuntimeSignalSummary fromRuntimeReport(JSONObject runtimeReport) {
        JSONObject rx = runtimeReport == null ? null : runtimeReport.optJSONObject("rx");
        JSONObject signal = rx == null ? null : rx.optJSONObject("signal");
        if (signal == null) {
            return EMPTY;
        }
        return new WfbRuntimeSignalSummary(
                WfbRuntimeSignalMetric.fromJson(signal.optJSONObject("rssi_dbm")),
                WfbRuntimeSignalMetric.fromJson(signal.optJSONObject("snr_db")),
                WfbRuntimeSignalMetric.fromJson(signal.optJSONObject("noise_dbm")));
    }

    public boolean hasRssi() {
        return rssiDbm.sampleCount > 0;
    }

    public boolean hasSnr() {
        return snrDb.sampleCount > 0;
    }
}
