//! Opt-in User Timing for browser profiling. No clocks enter simulation state.
pub(crate) struct Span {
    performance: web_sys::Performance,
    name: &'static str,
    start: String,
}

pub(crate) fn begin(name: &'static str) -> Option<Span> {
    let window = web_sys::window()?;
    let canvas = window.document()?.get_element_by_id("crabscape")?;
    if !canvas.has_attribute("data-cell-work-profile") {
        return None;
    }
    let performance = window.performance()?;
    let start = format!("{name}:start");
    performance.clear_marks_with_mark_name(&start);
    performance.clear_measures_with_measure_name(name);
    performance.mark(&start).ok()?;
    Some(Span {
        performance,
        name,
        start,
    })
}

impl Drop for Span {
    fn drop(&mut self) {
        let _ = self
            .performance
            .measure_with_start_mark(self.name, &self.start);
        self.performance.clear_marks_with_mark_name(&self.start);
    }
}
