## Approach

`RuntimeRadioSession::read_rx_packets` already owns endpoint selection, bulk-IN reading, parser iteration, and runtime counters. This change extends it to return `NeedMoreData` parser outcomes so diagnostic reporting can keep its existing `need_more_data` counter.

`bridge-run` will call the runtime helper once per receive poll and feed the returned packet outcomes into a new diagnostic processor. That processor mirrors `process_rx_buffer` behavior but consumes already-parsed packet results instead of parsing the same bytes again.

Timeouts remain classified through `RuntimeRadioError::timeout`, preserving the current bridge-run loop behavior.
