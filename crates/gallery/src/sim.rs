//! Simulated agent activity that drives the gallery.
//!
//! Fake token streams live here, outside the library, so components only
//! ever consume real [`StreamedContent`] values.

use mighty_gpui::stream::StreamedContent;
use std::time::Duration;

/// Interval between deterministic gallery simulation ticks.
pub const TICK_INTERVAL: Duration = Duration::from_millis(60);

const ANSWER: &str = "Based on the March pricing sheet, **Alpenrose Dairy** is the \
strongest option:\n\n\
- Unit cost is **7% lower** at your current volume\n\
- Delivery window is unchanged (Tue/Fri)\n\
- Net-30 terms match your existing supplier\n\n\
The one open question is cold-chain capacity in August — worth confirming \
before committing the full order.";

const CODE: &str = "fn cheapest(suppliers: &[Supplier]) -> Option<&Supplier> {\n\
    suppliers\n\
        .iter()\n\
        .filter(|s| s.in_stock)\n\
        .min_by(|a, b| a.unit_price.total_cmp(&b.unit_price))\n\
}";

/// How many characters arrive per tick.
const CHARS_PER_TICK: usize = 3;
/// Ticks to pause after both streams finish before restarting.
const RESTART_AFTER: usize = 60;

/// Visible categories changed by one simulation tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimulationDelta {
    answer_content: bool,
    answer_phase: bool,
    code_content: bool,
    code_phase: bool,
}

impl SimulationDelta {
    /// Whether the streamed answer text changed.
    pub fn answer_content_changed(self) -> bool {
        self.answer_content
    }

    /// Whether the answer entered or left its streaming phase.
    pub fn answer_phase_changed(self) -> bool {
        self.answer_phase
    }

    /// Whether the streamed code text changed.
    pub fn code_content_changed(self) -> bool {
        self.code_content
    }

    /// Whether the code entered or left its streaming phase.
    pub fn code_phase_changed(self) -> bool {
        self.code_phase
    }
}

/// The gallery's fake agent: two token streams that loop forever.
pub struct Simulation {
    /// The streamed markdown answer.
    pub answer: StreamedContent,
    /// The streamed code snippet.
    pub code: StreamedContent,
    answer_pos: usize,
    code_pos: usize,
    idle_ticks: usize,
    elapsed: Duration,
}

impl Simulation {
    /// Creates the simulation with both streams empty.
    pub fn new() -> Self {
        Self {
            answer: StreamedContent::new(),
            code: StreamedContent::new(),
            answer_pos: 0,
            code_pos: 0,
            idle_ticks: 0,
            elapsed: Duration::ZERO,
        }
    }

    /// Elapsed gallery time derived from simulation ticks.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Overall progress of the answer stream in `0.0..=1.0` — reused by
    /// stories that need a progress value (image generation).
    pub fn progress(&self) -> f32 {
        self.answer_pos as f32 / ANSWER.len() as f32
    }

    /// Advances the simulation by one tick.
    pub fn tick(&mut self) -> SimulationDelta {
        let answer_pos_before = self.answer_pos;
        let code_pos_before = self.code_pos;
        let answer_streaming_before = self.answer.is_streaming();
        let code_streaming_before = self.code.is_streaming();
        self.elapsed = self.elapsed.saturating_add(TICK_INTERVAL);
        let answer_done = advance(ANSWER, &mut self.answer_pos, &mut self.answer);
        // The code stream starts once the answer is halfway through.
        let code_done = if self.answer_pos * 2 >= ANSWER.len() {
            advance(CODE, &mut self.code_pos, &mut self.code)
        } else {
            false
        };

        if answer_done && code_done {
            self.idle_ticks += 1;
            if self.idle_ticks > RESTART_AFTER {
                let elapsed = self.elapsed;
                *self = Self::new();
                self.elapsed = elapsed;
            }
        }

        SimulationDelta {
            answer_content: self.answer_pos != answer_pos_before,
            answer_phase: self.answer.is_streaming() != answer_streaming_before,
            code_content: self.code_pos != code_pos_before,
            code_phase: self.code.is_streaming() != code_streaming_before,
        }
    }
}

/// Feeds the next few characters of `target` into `content`; returns whether
/// the stream is finished.
fn advance(target: &str, pos: &mut usize, content: &mut StreamedContent) -> bool {
    if *pos >= target.len() {
        return true;
    }
    let mut end = *pos;
    for _ in 0..CHARS_PER_TICK {
        match target[end..].chars().next() {
            Some(c) => end += c.len_utf8(),
            None => break,
        }
    }
    content.append(&target[*pos..end]);
    *pos = end;
    if *pos >= target.len() {
        content.finish();
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::Simulation;
    use std::time::Duration;

    #[test]
    fn elapsed_time_advances_with_simulation_ticks() {
        let mut simulation = Simulation::new();
        assert_eq!(simulation.elapsed(), Duration::ZERO);

        let delta = simulation.tick();

        assert_eq!(simulation.elapsed(), Duration::from_millis(60));
        assert!(delta.answer_content_changed());
        assert!(!delta.answer_phase_changed());
        assert!(!delta.code_content_changed());
    }

    #[test]
    fn completing_the_answer_reports_a_phase_change() {
        let mut simulation = Simulation::new();
        let mut completion_reported = false;

        while simulation.answer.is_streaming() {
            completion_reported |= simulation.tick().answer_phase_changed();
        }

        assert!(completion_reported);
    }
}
