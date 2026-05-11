package com.arcedge.wfblink.sdk;

import android.content.Context;
import java.io.File;

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

        public WfbManagedStreamsConfig build() {
            return new WfbManagedStreamsConfig(this);
        }
    }

    boolean keyFileExists() {
        return keyPath != null && new File(keyPath).isFile();
    }
}
