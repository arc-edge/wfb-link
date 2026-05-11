package com.arcedge.wfblink.sdk;

/** Typed Android SDK integration failure. */
public final class WfbLinkException extends Exception {
    public final String code;

    public WfbLinkException(String code, String message) {
        super(message);
        this.code = code;
    }

    public WfbLinkException(String code, String message, Throwable cause) {
        super(message, cause);
        this.code = code;
    }
}
