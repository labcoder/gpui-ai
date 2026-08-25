use gpui_ai::prelude::*;

// The application owns the text and the lifecycle. It advances them from
// whatever its model client hands back; the component renders the snapshot and
// never asks for more.
struct Reply {
    answer: StreamedContent,
}

impl Reply {
    fn started() -> Self {
        Self { answer: StreamedContent::new() }
    }

    fn token(&mut self, chunk: &str) {
        self.answer.append(chunk);
    }

    fn finished(&mut self) {
        self.answer.finish();
    }

    fn refused(&mut self, why: &str) {
        self.answer.fail(why);
    }
}
