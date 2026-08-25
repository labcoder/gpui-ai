use gpui_ai::cues::{self, Cue};

// One place to hang sound, haptics, or a notification. The library never plays
// anything itself: it says what happened and leaves the decision here.
let _cues = cues::observe(cx, |cue, _cx| match cue {
    Cue::ResponseSettled { succeeded: true, .. } => play("chime"),
    Cue::Copied => play("tick"),
    _ => {}
});
// Dropping the subscription stops the cues.
