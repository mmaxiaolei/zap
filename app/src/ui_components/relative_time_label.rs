//! Orca-style relative-time label that refreshes itself via `repaint_after`.
//!
//! `repaint_after` only re-runs layout/paint on the existing element tree (it does
//! **not** call `View::render`). The label string is therefore recomputed here in
//! `layout`, matching [`BrailleSpinner`] / [`BreathingRing`].
//!
//! Idle/Done rows have no spinner, so this element is what keeps `now` → `Nm` →
//! `Nh` → `Nd` aging. InProgress already paints often via the status spinner; this
//! element still schedules the next *label* boundary so time stays correct after
//! the spinner stops.

use std::time::Duration;

use instant::Instant;
use pathfinder_color::ColorU;
use pathfinder_geometry::vector::Vector2F;
use warpui::elements::{Element, Point, Text};
use warpui::event::DispatchedEvent;
use warpui::fonts::FamilyId;
use warpui::{
    AfterLayoutContext, AppContext, EventContext, LayoutContext, PaintContext, SizeConstraint,
};

use crate::util::time_format::{
    duration_until_next_short_time_ago_change, format_short_time_ago_since,
};

pub(crate) struct RelativeTimeLabel {
    since: Instant,
    color: ColorU,
    family_id: FamilyId,
    font_size: f32,
    inner: Option<Text>,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl RelativeTimeLabel {
    pub(crate) fn new(
        since: Instant,
        family_id: FamilyId,
        font_size: f32,
        color: impl Into<ColorU>,
    ) -> Self {
        Self {
            since,
            color: color.into(),
            family_id,
            font_size,
            inner: None,
            size: None,
            origin: None,
        }
    }
}

impl Element for RelativeTimeLabel {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        ctx: &mut LayoutContext,
        app: &AppContext,
    ) -> Vector2F {
        let label = format_short_time_ago_since(self.since);
        let mut text =
            Text::new_inline(label, self.family_id, self.font_size).with_color(self.color);
        let size = text.layout(constraint, ctx, app);
        self.inner = Some(text);
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, ctx: &mut AfterLayoutContext, app: &AppContext) {
        if let Some(t) = self.inner.as_mut() {
            t.after_layout(ctx, app);
        }
    }

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        if let Some(t) = self.inner.as_mut() {
            t.paint(origin, ctx, app);
        }
        // Schedule the next paint at the next label boundary (≤60s while in
        // `now`/`Nm`, up to 1h/1d later for coarser buckets). Presenter keeps the
        // earliest of all `repaint_after` callers, so multiple agent rows share one
        // timer without thrashing Idle trees every frame.
        let delay: Duration = duration_until_next_short_time_ago_change(self.since);
        ctx.repaint_after(delay);
    }

    fn size(&self) -> Option<Vector2F> {
        self.size
    }

    fn origin(&self) -> Option<Point> {
        self.origin
    }

    fn dispatch_event(
        &mut self,
        _: &DispatchedEvent,
        _: &mut EventContext,
        _: &AppContext,
    ) -> bool {
        false
    }
}
