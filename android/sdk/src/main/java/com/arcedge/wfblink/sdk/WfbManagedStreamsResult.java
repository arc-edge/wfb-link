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
    public final WfbManagedDirectionStats uplink;
    public final WfbManagedDirectionStats downlink;
    public final WfbManagedHelperStatus helperStatus;
    public final WfbRuntimeSignalSummary rxSignal;
    public final WfbManagedStreamsHealth health;

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
            String runtimeReportJson,
            WfbManagedDirectionStats uplink,
            WfbManagedDirectionStats downlink,
            WfbRuntimeSignalSummary rxSignal) {
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
        this.uplink = uplink;
        this.downlink = downlink;
        this.helperStatus = new WfbManagedHelperStatus(txHelperStatus, rxHelperStatus);
        this.rxSignal = rxSignal;
        this.health =
                new WfbManagedStreamsHealth(
                        ok,
                        code,
                        message,
                        runtimeResult,
                        stopReason,
                        this.helperStatus,
                        uplink,
                        downlink,
                        rxSignal);
    }

    static WfbManagedStreamsResult fromJson(String json) throws WfbLinkException {
        try {
            JSONObject value = new JSONObject(json);
            String runtimeReportJson =
                    value.isNull("runtime_report_json")
                            ? null
                            : value.optString("runtime_report_json", null);
            JSONObject runtimeReport = parseRuntimeReport(runtimeReportJson);
            JSONObject tx = runtimeReport == null ? null : runtimeReport.optJSONObject("tx");
            JSONObject rx = runtimeReport == null ? null : runtimeReport.optJSONObject("rx");
            WfbManagedDirectionStats uplink =
                    new WfbManagedDirectionStats(
                            WfbStreamDirection.TX,
                            value.optLong("raw_tx_packets", 0),
                            value.optLong("raw_tx_bytes", 0),
                            value.optLong("tx_datagrams", 0),
                            value.optLong("submitted_frames", 0),
                            optLong(tx, "failed_submissions", 0),
                            optLong(tx, "dropped_datagrams", 0),
                            0,
                            0,
                            optLong(tx, "bytes_written", 0));
            WfbManagedDirectionStats downlink =
                    new WfbManagedDirectionStats(
                            WfbStreamDirection.RX,
                            value.optLong("raw_rx_packets", 0),
                            value.optLong("raw_rx_bytes", 0),
                            0,
                            0,
                            0,
                            0,
                            value.optLong("rx_frames", 0),
                            value.optLong("rx_forwarded_payloads", 0),
                            0);
            WfbRuntimeSignalSummary rxSignal = WfbRuntimeSignalSummary.fromRuntimeReport(runtimeReport);
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
                    runtimeReportJson,
                    uplink,
                    downlink,
                    rxSignal);
        } catch (JSONException error) {
            throw new WfbLinkException("android_sdk_invalid_native_json", error.getMessage(), error);
        }
    }

    private static JSONObject parseRuntimeReport(String json) throws JSONException {
        if (json == null || json.length() == 0) {
            return null;
        }
        return new JSONObject(json);
    }

    private static long optLong(JSONObject value, String name, long fallback) {
        return value == null ? fallback : value.optLong(name, fallback);
    }
}
