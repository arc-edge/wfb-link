package com.arcedge.wfblink.sdk;

import org.json.JSONObject;

/** Min/max/average signal metric parsed from the runtime RX report. */
public final class WfbRuntimeSignalMetric {
    public static final WfbRuntimeSignalMetric EMPTY =
            new WfbRuntimeSignalMetric(0, null, null, null, null);

    public final long sampleCount;
    public final Long last;
    public final Long min;
    public final Long max;
    public final Long average;

    WfbRuntimeSignalMetric(long sampleCount, Long last, Long min, Long max, Long average) {
        this.sampleCount = sampleCount;
        this.last = last;
        this.min = min;
        this.max = max;
        this.average = average;
    }

    static WfbRuntimeSignalMetric fromJson(JSONObject value) {
        if (value == null) {
            return EMPTY;
        }
        return new WfbRuntimeSignalMetric(
                value.optLong("sample_count", 0),
                optionalLong(value, "last"),
                optionalLong(value, "min"),
                optionalLong(value, "max"),
                optionalLong(value, "average"));
    }

    private static Long optionalLong(JSONObject value, String name) {
        if (!value.has(name) || value.isNull(name)) {
            return null;
        }
        return Long.valueOf(value.optLong(name));
    }
}
