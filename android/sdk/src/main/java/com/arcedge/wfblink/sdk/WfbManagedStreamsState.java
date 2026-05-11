package com.arcedge.wfblink.sdk;

/** Lifecycle state for an Android managed WFB stream session. */
public enum WfbManagedStreamsState {
    CREATED,
    RUNNING,
    STOP_REQUESTED,
    SUCCEEDED,
    FAILED
}
