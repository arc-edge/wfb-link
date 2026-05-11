package com.arcedge.wfblink.sdk;

import org.json.JSONException;
import org.json.JSONObject;

/** Structured result from a blocking managed WFB stream session. */
public final class WfbManagedStreamsResult {
    public final boolean ok;
    public final String code;
    public final String message;
    public final long submittedFrames;
    public final long txDatagrams;
    public final long rawTxPackets;
    public final long rawTxBytes;
    public final long rawRxPackets;
    public final long rawRxBytes;
    public final long rxFrames;
    public final long rxForwardedPayloads;
    public final String runtimeResult;
    public final String stopReason;
    public final String txHelperStatus;
    public final String rxHelperStatus;
    public final String runtimeReportJson;

    private WfbManagedStreamsResult(
            boolean ok,
            String code,
            String message,
            long submittedFrames,
            long txDatagrams,
            long rawTxPackets,
            long rawTxBytes,
            long rawRxPackets,
            long rawRxBytes,
            long rxFrames,
            long rxForwardedPayloads,
            String runtimeResult,
            String stopReason,
            String txHelperStatus,
            String rxHelperStatus,
            String runtimeReportJson) {
        this.ok = ok;
        this.code = code;
        this.message = message;
        this.submittedFrames = submittedFrames;
        this.txDatagrams = txDatagrams;
        this.rawTxPackets = rawTxPackets;
        this.rawTxBytes = rawTxBytes;
        this.rawRxPackets = rawRxPackets;
        this.rawRxBytes = rawRxBytes;
        this.rxFrames = rxFrames;
        this.rxForwardedPayloads = rxForwardedPayloads;
        this.runtimeResult = runtimeResult;
        this.stopReason = stopReason;
        this.txHelperStatus = txHelperStatus;
        this.rxHelperStatus = rxHelperStatus;
        this.runtimeReportJson = runtimeReportJson;
    }

    static WfbManagedStreamsResult fromJson(String json) throws WfbLinkException {
        try {
            JSONObject value = new JSONObject(json);
            return new WfbManagedStreamsResult(
                    value.optBoolean("ok", false),
                    value.optString("code", "unknown"),
                    value.optString("message", ""),
                    value.optLong("submitted_frames", 0),
                    value.optLong("tx_datagrams", 0),
                    value.optLong("raw_tx_packets", 0),
                    value.optLong("raw_tx_bytes", 0),
                    value.optLong("raw_rx_packets", 0),
                    value.optLong("raw_rx_bytes", 0),
                    value.optLong("rx_frames", 0),
                    value.optLong("rx_forwarded_payloads", 0),
                    value.optString("runtime_result", "unknown"),
                    value.optString("stop_reason", "unknown"),
                    value.optString("tx_helper_status", "unknown"),
                    value.optString("rx_helper_status", "unknown"),
                    value.isNull("runtime_report_json")
                            ? null
                            : value.optString("runtime_report_json", null));
        } catch (JSONException error) {
            throw new WfbLinkException("android_sdk_invalid_native_json", error.getMessage(), error);
        }
    }
}
