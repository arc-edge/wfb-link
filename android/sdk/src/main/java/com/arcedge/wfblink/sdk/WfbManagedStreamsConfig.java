package com.arcedge.wfblink.sdk;

import android.content.Context;
import java.io.File;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Configuration for one blocking Android USBHost managed WFB stream session. */
public final class WfbManagedStreamsConfig {
    public final WfbUsbHandoff usb;
    public final String nativeLibraryDir;
    public final String workingDir;
    public final String keyPath;
    public final String firmwarePath;
    public final String macTablePath;
    public final String bbTablePath;
    public final String rfTablePath;
    public final int channelNumber;
    public final int timeoutMs;
    public final int durationMs;
    public final int payloadCount;
    public final int linkId;
    public final int uplinkRadioPort;
    public final int downlinkRadioPort;
    public final int runtimeBindPort;
    public final int txBindPort;
    public final int rawTxPort;
    public final int rxAggregatorPort;
    public final int rawRxPort;
    public final int rawPayloadBytes;
    public final int txBandwidthMhz;
    public final int txMcs;
    public final int txFecK;
    public final int txFecN;
    public final List<WfbManagedStream> streams;

    private WfbManagedStreamsConfig(Builder builder) {
        this.usb = builder.usb;
        this.nativeLibraryDir = builder.nativeLibraryDir;
        this.workingDir = builder.workingDir;
        this.keyPath = builder.keyPath;
        this.firmwarePath = builder.firmwarePath;
        this.macTablePath = builder.macTablePath;
        this.bbTablePath = builder.bbTablePath;
        this.rfTablePath = builder.rfTablePath;
        this.channelNumber = builder.channelNumber;
        this.timeoutMs = builder.timeoutMs;
        this.durationMs = builder.durationMs;
        this.payloadCount = builder.payloadCount;
        this.linkId = builder.linkId;
        this.uplinkRadioPort = builder.uplinkRadioPort;
        this.downlinkRadioPort = builder.downlinkRadioPort;
        this.runtimeBindPort = builder.runtimeBindPort;
        this.txBindPort = builder.txBindPort;
        this.rawTxPort = builder.rawTxPort;
        this.rxAggregatorPort = builder.rxAggregatorPort;
        this.rawRxPort = builder.rawRxPort;
        this.rawPayloadBytes = builder.rawPayloadBytes;
        this.txBandwidthMhz = builder.txBandwidthMhz;
        this.txMcs = builder.txMcs;
        this.txFecK = builder.txFecK;
        this.txFecN = builder.txFecN;
        this.streams = Collections.unmodifiableList(new ArrayList<WfbManagedStream>(builder.streams));
    }

    public static Builder builder(Context context, WfbUsbHandoff usb) {
        return new Builder(context, usb);
    }

    public static final class Builder {
        private WfbUsbHandoff usb;
        private String nativeLibraryDir;
        private String workingDir;
        private String keyPath;
        private String firmwarePath;
        private String macTablePath;
        private String bbTablePath;
        private String rfTablePath;
        private int channelNumber = 36;
        private int timeoutMs = 5000;
        private int durationMs = 12000;
        private int payloadCount = 20;
        private int linkId = 1;
        private int uplinkRadioPort = 6;
        private int downlinkRadioPort = 4;
        private int runtimeBindPort = 15700;
        private int txBindPort = 15706;
        private int rawTxPort = 15606;
        private int rxAggregatorPort = 15804;
        private int rawRxPort = 15904;
        private int rawPayloadBytes = 512;
        private int txBandwidthMhz = WfbManagedTxProfile.DEFAULT_BANDWIDTH_MHZ;
        private int txMcs = WfbManagedTxProfile.DEFAULT_MCS;
        private int txFecK = WfbManagedTxProfile.DEFAULT_FEC_K;
        private int txFecN = WfbManagedTxProfile.DEFAULT_FEC_N;
        private final List<WfbManagedStream> streams = new ArrayList<WfbManagedStream>();

        private Builder(Context context, WfbUsbHandoff usb) {
            this.usb = usb;
            this.nativeLibraryDir = context.getApplicationInfo().nativeLibraryDir;
            this.workingDir = context.getFilesDir().getAbsolutePath();
        }

        public Builder keyPath(String value) {
            this.keyPath = value;
            return this;
        }

        public Builder initAssets(String firmware, String macTable, String bbTable, String rfTable) {
            this.firmwarePath = firmware;
            this.macTablePath = macTable;
            this.bbTablePath = bbTable;
            this.rfTablePath = rfTable;
            return this;
        }

        public Builder nativeLibraryDir(String value) {
            this.nativeLibraryDir = value;
            return this;
        }

        public Builder workingDir(String value) {
            this.workingDir = value;
            return this;
        }

        public Builder channelNumber(int value) {
            this.channelNumber = value;
            return this;
        }

        public Builder timeoutMs(int value) {
            this.timeoutMs = value;
            return this;
        }

        public Builder durationMs(int value) {
            this.durationMs = value;
            return this;
        }

        public Builder payloadCount(int value) {
            this.payloadCount = value;
            return this;
        }

        public Builder linkId(int value) {
            this.linkId = value;
            return this;
        }

        public Builder radioPorts(int uplink, int downlink) {
            this.uplinkRadioPort = uplink;
            this.downlinkRadioPort = downlink;
            return this;
        }

        public Builder udpPorts(
                int runtimeBind,
                int txBind,
                int rawTx,
                int rxAggregator,
                int rawRx) {
            this.runtimeBindPort = runtimeBind;
            this.txBindPort = txBind;
            this.rawTxPort = rawTx;
            this.rxAggregatorPort = rxAggregator;
            this.rawRxPort = rawRx;
            return this;
        }

        public Builder rawPayloadBytes(int value) {
            this.rawPayloadBytes = value;
            return this;
        }

        public Builder txProfile(WfbManagedTxProfile value) {
            if (value != null) {
                this.txBandwidthMhz = value.bandwidthMhz;
                this.txMcs = value.mcs;
                this.txFecK = value.fecK;
                this.txFecN = value.fecN;
            }
            return this;
        }

        public Builder addStream(WfbManagedStream value) {
            this.streams.add(value);
            return this;
        }

        public Builder streams(List<WfbManagedStream> values) {
            this.streams.clear();
            if (values != null) {
                this.streams.addAll(values);
            }
            return this;
        }

        public WfbManagedStreamsConfig build() {
            applySupportedNamedStreamMapping();
            return new WfbManagedStreamsConfig(this);
        }

        private void applySupportedNamedStreamMapping() {
            if (streams.isEmpty() || streams.size() != 2) {
                return;
            }
            Set<String> names = new HashSet<String>();
            Set<Integer> localPorts = new HashSet<Integer>();
            WfbManagedStream tx = null;
            WfbManagedStream rx = null;
            for (WfbManagedStream stream : streams) {
                if (stream == null
                        || stream.name == null
                        || stream.direction == null
                        || stream.payloadKind != WfbPayloadKind.RAW_APPLICATION_DATAGRAM
                        || !names.add(stream.name)
                        || !localPorts.add(Integer.valueOf(stream.localUdpPort))) {
                    return;
                }
                if (stream.direction == WfbStreamDirection.TX) {
                    if (tx != null) {
                        return;
                    }
                    tx = stream;
                } else if (stream.direction == WfbStreamDirection.RX) {
                    if (rx != null) {
                        return;
                    }
                    rx = stream;
                }
            }
            if (tx == null || rx == null || tx.linkId != rx.linkId) {
                return;
            }

            this.linkId = tx.linkId;
            this.uplinkRadioPort = tx.radioPort;
            this.downlinkRadioPort = rx.radioPort;
            this.rawTxPort = tx.localUdpPort;
            this.rawRxPort = rx.localUdpPort;
            if (tx.txProfile != null) {
                txProfile(tx.txProfile);
            }
        }
    }

    boolean keyFileExists() {
        return keyPath != null && new File(keyPath).isFile();
    }
}
