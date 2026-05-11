package com.arcedge.wfblink.sdk;

import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Future;
import java.util.concurrent.atomic.AtomicReference;

/** Handle for an Android managed WFB stream session running on an app executor. */
public final class WfbManagedStreamsSession {
    private final WfbLinkManager manager;
    private final WfbManagedStreamsConfig config;
    private final WfbManagedStreamsCallback callback;
    private final AtomicReference<WfbManagedStreamsStatus> status =
            new AtomicReference<WfbManagedStreamsStatus>(WfbManagedStreamsStatus.created());
    private Future<?> future;

    WfbManagedStreamsSession(
            WfbLinkManager manager,
            WfbManagedStreamsConfig config,
            ExecutorService executor,
            WfbManagedStreamsCallback callback)
            throws WfbLinkException {
        this.manager = manager;
        this.config = config;
        this.callback = callback == null ? WfbManagedStreamsCallback.NO_OP : callback;
        start(executor);
    }

    private synchronized void start(ExecutorService executor) throws WfbLinkException {
        if (executor == null) {
            throw new WfbLinkException("android_sdk_executor_missing", "executor is required");
        }
        if (future != null) {
            throw new WfbLinkException(
                    "android_sdk_session_already_started", "managed session already started");
        }
        updateStatus(status.get().asRunning(System.currentTimeMillis()));
        try {
            future =
                    executor.submit(
                            new Runnable() {
                                @Override
                                public void run() {
                                    runBlockingToTerminalStatus();
                                }
                            });
        } catch (RuntimeException error) {
            updateStatus(
                    status.get()
                            .withError(
                                    new WfbLinkException(
                                            "android_sdk_executor_rejected",
                                            error.getMessage() == null
                                                    ? error.toString()
                                                    : error.getMessage(),
                                            error),
                                    System.currentTimeMillis()));
            throw new WfbLinkException(
                    "android_sdk_executor_rejected", "executor rejected managed session", error);
        }
    }

    public WfbManagedStreamsStatus status() {
        return status.get();
    }

    public boolean isRunning() {
        return status.get().isRunning();
    }

    public boolean isTerminal() {
        return status.get().isTerminal();
    }

    public boolean requestStop() {
        while (true) {
            WfbManagedStreamsStatus previous = status.get();
            if (previous.isTerminal()) {
                return false;
            }
            WfbManagedStreamsStatus next = previous.withStopRequested();
            if (status.compareAndSet(previous, next)) {
                notifyStatus(next);
                return true;
            }
        }
    }

    public WfbManagedStreamsResult await() throws WfbLinkException, InterruptedException {
        Future<?> running;
        synchronized (this) {
            running = future;
        }
        if (running == null) {
            throw new WfbLinkException("android_sdk_session_not_started", "session is not started");
        }
        try {
            running.get();
        } catch (ExecutionException error) {
            throw new WfbLinkException(
                    "android_sdk_session_failed", "managed session task failed", error);
        }
        WfbManagedStreamsStatus finalStatus = status.get();
        if (finalStatus.result != null) {
            return finalStatus.result;
        }
        if (finalStatus.error != null) {
            throw finalStatus.error;
        }
        throw new WfbLinkException(
                "android_sdk_session_incomplete", "managed session finished without a result");
    }

    private void runBlockingToTerminalStatus() {
        try {
            WfbManagedStreamsResult result = manager.runManagedStreamsBlocking(config);
            WfbManagedStreamsStatus next =
                    status.get().withResult(result, System.currentTimeMillis());
            updateStatus(next);
            safeCompleted(result);
        } catch (WfbLinkException error) {
            WfbManagedStreamsStatus next =
                    status.get().withError(error, System.currentTimeMillis());
            updateStatus(next);
            safeFailed(error);
        } catch (RuntimeException error) {
            WfbLinkException wrapped =
                    new WfbLinkException(
                            "android_sdk_runtime_exception",
                            error.getMessage() == null ? error.toString() : error.getMessage(),
                            error);
            WfbManagedStreamsStatus next =
                    status.get().withError(wrapped, System.currentTimeMillis());
            updateStatus(next);
            safeFailed(wrapped);
        }
    }

    private void updateStatus(WfbManagedStreamsStatus value) {
        status.set(value);
        notifyStatus(value);
    }

    private void notifyStatus(WfbManagedStreamsStatus value) {
        try {
            callback.onStatusChanged(value);
        } catch (RuntimeException ignored) {
        }
    }

    private void safeCompleted(WfbManagedStreamsResult result) {
        try {
            callback.onCompleted(result);
        } catch (RuntimeException ignored) {
        }
    }

    private void safeFailed(WfbLinkException error) {
        try {
            callback.onFailed(error);
        } catch (RuntimeException ignored) {
        }
    }
}
