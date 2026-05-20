package com.arcedge.wfblink.sdk;

import org.json.JSONObject;

/** Source and RF context for runtime RX signal metadata. */
public final class WfbRuntimeSignalMetadata {
    public static final WfbRuntimeSignalMetadata EMPTY =
            new WfbRuntimeSignalMetadata(
                    null, null, 0, 0, 0, 0, null, null, null, null, null, null);

    public final String rssiSource;
    public final String snrSource;
    public final long phyStatusFrames;
    public final long rssiValidFrames;
    public final long snrFrames;
    public final long noiseFrames;
    public final Long lastChannel;
    public final Long lastFrequencyMhz;
    public final String lastBand;
    public final Long lastBandwidthMhz;
    public final Long lastMcsIndex;
    public final String lastRxRate;

    WfbRuntimeSignalMetadata(
            String rssiSource,
            String snrSource,
            long phyStatusFrames,
            long rssiValidFrames,
            long snrFrames,
            long noiseFrames,
            Long lastChannel,
            Long lastFrequencyMhz,
            String lastBand,
            Long lastBandwidthMhz,
            Long lastMcsIndex,
            String lastRxRate) {
        this.rssiSource = rssiSource;
        this.snrSource = snrSource;
        this.phyStatusFrames = phyStatusFrames;
        this.rssiValidFrames = rssiValidFrames;
        this.snrFrames = snrFrames;
        this.noiseFrames = noiseFrames;
        this.lastChannel = lastChannel;
        this.lastFrequencyMhz = lastFrequencyMhz;
        this.lastBand = lastBand;
        this.lastBandwidthMhz = lastBandwidthMhz;
        this.lastMcsIndex = lastMcsIndex;
        this.lastRxRate = lastRxRate;
    }

    static WfbRuntimeSignalMetadata fromJson(JSONObject value) {
        if (value == null) {
            return EMPTY;
        }
        return new WfbRuntimeSignalMetadata(
                optionalString(value, "rssi_source"),
                optionalString(value, "snr_source"),
                value.optLong("phy_status_frames", 0),
                value.optLong("rssi_valid_frames", 0),
                value.optLong("snr_frames", 0),
                value.optLong("noise_frames", 0),
                optionalLong(value, "last_channel"),
                optionalLong(value, "last_frequency_mhz"),
                optionalString(value, "last_band"),
                optionalLong(value, "last_bandwidth_mhz"),
                optionalLong(value, "last_mcs_index"),
                optionalRxRate(value.opt("last_rx_rate")));
    }

    private static Long optionalLong(JSONObject value, String name) {
        if (!value.has(name) || value.isNull(name)) {
            return null;
        }
        return Long.valueOf(value.optLong(name));
    }

    private static String optionalString(JSONObject value, String name) {
        if (!value.has(name) || value.isNull(name)) {
            return null;
        }
        return value.optString(name, null);
    }

    private static String optionalRxRate(Object value) {
        if (value == null || value == JSONObject.NULL) {
            return null;
        }
        return value.toString();
    }
}
