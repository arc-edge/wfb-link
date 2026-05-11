package com.arcedge.wfblink.sdk;

/** WFB-NG TX helper profile for a managed raw application stream. */
public final class WfbManagedTxProfile {
    public static final int DEFAULT_BANDWIDTH_MHZ = 20;
    public static final int DEFAULT_MCS = 0;
    public static final int DEFAULT_FEC_K = 2;
    public static final int DEFAULT_FEC_N = 4;

    public final int bandwidthMhz;
    public final int mcs;
    public final int fecK;
    public final int fecN;

    private WfbManagedTxProfile(Builder builder) {
        this.bandwidthMhz = builder.bandwidthMhz;
        this.mcs = builder.mcs;
        this.fecK = builder.fecK;
        this.fecN = builder.fecN;
    }

    public static WfbManagedTxProfile defaults() {
        return builder().build();
    }

    public static WfbManagedTxProfile of(int bandwidthMhz, int mcs, int fecK, int fecN) {
        return builder().bandwidthMhz(bandwidthMhz).mcs(mcs).fec(fecK, fecN).build();
    }

    public static Builder builder() {
        return new Builder();
    }

    public static final class Builder {
        private int bandwidthMhz = DEFAULT_BANDWIDTH_MHZ;
        private int mcs = DEFAULT_MCS;
        private int fecK = DEFAULT_FEC_K;
        private int fecN = DEFAULT_FEC_N;

        public Builder bandwidthMhz(int value) {
            this.bandwidthMhz = value;
            return this;
        }

        public Builder mcs(int value) {
            this.mcs = value;
            return this;
        }

        public Builder fec(int k, int n) {
            this.fecK = k;
            this.fecN = n;
            return this;
        }

        public WfbManagedTxProfile build() {
            return new WfbManagedTxProfile(this);
        }
    }
}
