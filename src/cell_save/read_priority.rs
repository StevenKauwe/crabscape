//! Fairness between page reads and urgent cellular capture attempts.

pub(super) fn may_start(
    has_requests: bool,
    read_busy: bool,
    save_busy: bool,
    save_due: bool,
    read_backoff: bool,
    after_capture_failure: bool,
) -> bool {
    has_requests && !read_busy && !read_backoff && (save_busy || !save_due || after_capture_failure)
}

#[cfg(test)]
mod tests {
    use super::may_start;

    #[test]
    fn repeated_urgent_capture_waits_allow_each_requested_batch_a_turn() {
        // Ordinarily a due save goes first. If it cannot capture, each
        // subsequent failure yields one batch even while flush stays urgent.
        assert!(!may_start(true, false, false, true, false, false));
        for _ in 0..4 {
            assert!(may_start(true, false, false, true, false, true));
            // Starting a read consumes its turn; the next capture can run.
            assert!(!may_start(true, false, false, true, false, false));
        }
    }

    #[test]
    fn absent_or_failed_reads_do_not_hold_up_the_next_capture() {
        // No request means no read is started even after a capture failure.
        assert!(!may_start(false, false, false, true, false, true));
        // A failed read is backed off and cannot start again during that
        // interval; the caller may still retry capture normally.
        assert!(!may_start(true, false, false, true, true, true));
        assert!(may_start(true, false, false, true, false, true));
        assert!(!may_start(true, true, false, true, false, true));
        // A sealed save may read while its worker compresses.
        assert!(may_start(true, false, true, true, false, false));
    }
}
