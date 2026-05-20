package com.arcedge.wfblink.sdk;

import org.json.JSONObject;

/** RX signal summary parsed from the runtime report when radio metadata exists. */
public final class WfbRuntimeSignalSummary {
    public static final WfbRuntimeSignalSummary EMPTY =
            new WfbRuntimeSignalSummary(
                    WfbRuntimeSignalMetric.EMPTY,
                    WfbRuntimeSignalMetric.EMPTY,
                    WfbRuntimeSignalMetric.EMPTY,
                    "unknown",
                    null,
                    "unknown",
                    "no_valid_samples",
                    null,
                    0,
                    WfbRuntimeSignalMetadata.EMPTY);

    public final WfbRuntimeSignalMetric rssiDbm;
    public final WfbRuntimeSignalMetric snrDb;
    public final WfbRuntimeSignalMetric noiseDbm;
    public final String state;
    public final Integer qualityLevel;
    public final String qualityLabel;
    public final String qualityBasis;
    public final Long lastSampleUnixMs;
    public final long staleAfterMs;
    public final WfbRuntimeSignalMetadata metadata;

    WfbRuntimeSignalSummary(
            WfbRuntimeSignalMetric rssiDbm,
            WfbRuntimeSignalMetric snrDb,
            WfbRuntimeSignalMetric noiseDbm,
            String state,
            Integer qualityLevel,
            String qualityLabel,
            String qualityBasis,
            Long lastSampleUnixMs,
            long staleAfterMs,
            WfbRuntimeSignalMetadata metadata) {
        this.rssiDbm = rssiDbm;
        this.snrDb = snrDb;
        this.noiseDbm = noiseDbm;
        this.state = state;
        this.qualityLevel = qualityLevel;
        this.qualityLabel = qualityLabel;
        this.qualityBasis = qualityBasis;
        this.lastSampleUnixMs = lastSampleUnixMs;
        this.staleAfterMs = staleAfterMs;
        this.metadata = metadata;
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
                WfbRuntimeSignalMetric.fromJson(signal.optJSONObject("noise_dbm")),
                signal.optString("state", "unknown"),
                optionalInteger(signal, "quality_level"),
                signal.optString("quality_label", "unknown"),
                signal.optString("quality_basis", "no_valid_samples"),
                optionalLong(signal, "last_sample_unix_ms"),
                signal.optLong("stale_after_ms", 0),
                WfbRuntimeSignalMetadata.fromJson(signal.optJSONObject("metadata")));
    }

    public boolean hasRssi() {
        return rssiDbm.sampleCount > 0;
    }

    public boolean hasSnr() {
        return snrDb.sampleCount > 0;
    }

    public boolean hasQuality() {
        return qualityLevel != null;
    }

    private static Long optionalLong(JSONObject value, String name) {
        if (!value.has(name) || value.isNull(name)) {
            return null;
        }
        return Long.valueOf(value.optLong(name));
    }

    private static Integer optionalInteger(JSONObject value, String name) {
        if (!value.has(name) || value.isNull(name)) {
            return null;
        }
        return Integer.valueOf(value.optInt(name));
    }
}
