use gpui_ai::prelude::*;

// Every event carries the application's own identifier rather than a row
// number. A list that reorders, filters, or loses a row while a decision is in
// flight still resolves to the thing it was about.
fn gate(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    ApprovalCard::new("deploy-2f9c", "Deploy build 2f9c to production")
        .decision(self.decisions.get("deploy-2f9c").copied().unwrap_or_default())
        .on_event(cx.listener(|this, event: &ApprovalEvent, _, cx| {
            let (id, decision) = match event {
                ApprovalEvent::Approved { id } | ApprovalEvent::ApprovedAlways { id } => {
                    (id.clone(), ApprovalDecision::Approved)
                }
                ApprovalEvent::Rejected { id } => (id.clone(), ApprovalDecision::Rejected),
            };
            this.decisions.insert(id, decision);
            cx.notify();
        }))
}
