package com.arcedge.wfblink.sdk;

/** Callback invoked from the caller-provided executor thread. */
public interface WfbManagedStreamsCallback {
    WfbManagedStreamsCallback NO_OP =
            new WfbManagedStreamsCallback() {
                @Override
                public void onStatusChanged(WfbManagedStreamsStatus status) {}

                @Override
                public void onCompleted(WfbManagedStreamsResult result) {}

                @Override
                public void onFailed(WfbLinkException error) {}
            };

    void onStatusChanged(WfbManagedStreamsStatus status);

    void onCompleted(WfbManagedStreamsResult result);

    void onFailed(WfbLinkException error);
}
