package com.arcedge.wfblink.sdk;

/** Named product-facing managed WFB stream. */
public final class WfbManagedStream {
    public final String name;
    public final WfbStreamDirection direction;
    public final int radioPort;
    public final int localUdpPort;
    public final int linkId;
    public final WfbPayloadKind payloadKind;
    public final WfbStreamCriticality criticality;
    public final WfbManagedTxProfile txProfile;

    private WfbManagedStream(Builder builder) {
        this.name = builder.name;
        this.direction = builder.direction;
        this.radioPort = builder.radioPort;
        this.localUdpPort = builder.localUdpPort;
        this.linkId = builder.linkId;
        this.payloadKind = builder.payloadKind;
        this.criticality = builder.criticality;
        this.txProfile = builder.txProfile;
    }

    public static Builder tx(String name, int radioPort, int localUdpPort) {
        return builder(name, WfbStreamDirection.TX, radioPort, localUdpPort);
    }

    public static Builder rx(String name, int radioPort, int localUdpPort) {
        return builder(name, WfbStreamDirection.RX, radioPort, localUdpPort);
    }

    public static Builder builder(
            String name, WfbStreamDirection direction, int radioPort, int localUdpPort) {
        return new Builder(name, direction, radioPort, localUdpPort);
    }

    public static final class Builder {
        private final String name;
        private final WfbStreamDirection direction;
        private final int radioPort;
        private final int localUdpPort;
        private int linkId = 1;
        private WfbPayloadKind payloadKind = WfbPayloadKind.RAW_APPLICATION_DATAGRAM;
        private WfbStreamCriticality criticality = WfbStreamCriticality.REQUIRED;
        private WfbManagedTxProfile txProfile = WfbManagedTxProfile.defaults();

        private Builder(
                String name, WfbStreamDirection direction, int radioPort, int localUdpPort) {
            this.name = name;
            this.direction = direction;
            this.radioPort = radioPort;
            this.localUdpPort = localUdpPort;
        }

        public Builder linkId(int value) {
            this.linkId = value;
            return this;
        }

        public Builder payloadKind(WfbPayloadKind value) {
            this.payloadKind = value;
            return this;
        }

        public Builder criticality(WfbStreamCriticality value) {
            this.criticality = value;
            return this;
        }

        public Builder txProfile(WfbManagedTxProfile value) {
            this.txProfile = value;
            return this;
        }

        public WfbManagedStream build() {
            return new WfbManagedStream(this);
        }
    }
}
