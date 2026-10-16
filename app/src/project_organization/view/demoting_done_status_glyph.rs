//! Done residual status glyph that flips to Idle after [`DONE_RESIDUAL_TTL`].
//!
//! Like [`RelativeTimeLabel`], this recomputes in `layout` and schedules
//! `repaint_after` for the demotion boundary so Idle/Done rows update without a
//! full `View::render` (repaint only re-runs the existing element tree).

use instant::Instant;
use pathfinder_color::ColorU;
use pathfinder_geometry::vector::Vector2F;
use warp_core::ui::icons::Icon as WarpIcon;
use warp_core::ui::theme::Fill as ThemeFill;
use warpui::elements::{
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Element, Empty, Flex,
    MainAxisAlignment, MainAxisSize, ParentElement, Point, Radius,
};
use warpui::event::DispatchedEvent;
use warpui::{
    AfterLayoutContext, AppContext, EventContext, LayoutContext, PaintContext, SizeConstraint,
};

use crate::project_organization::workspace_agent_activity::{
    demote_done_phase, duration_until_done_demotion, WorkspaceAgentPhase,
};

const IDLE_DOT_SIZE: f32 = 6.;
const DONE_CHECK_SIZE: f32 = 11.;

pub(crate) struct DemotingDoneStatusGlyph {
    phase_changed_at: Instant,
    idle_color: ColorU,
    done_check_color: ColorU,
    slot_size: f32,
    inner: Option<Box<dyn Element>>,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl DemotingDoneStatusGlyph {
    pub(crate) fn new(
        phase_changed_at: Instant,
        idle_color: ColorU,
        done_check_color: ColorU,
        slot_size: f32,
    ) -> Self {
        Self {
            phase_changed_at,
            idle_color,
            done_check_color,
            slot_size,
            inner: None,
            size: None,
            origin: None,
        }
    }

    fn build_glyph(&self, phase: WorkspaceAgentPhase) -> Box<dyn Element> {
        match phase {
            WorkspaceAgentPhase::Idle => ConstrainedBox::new(
                Container::new(Empty::new().finish())
                    .with_background(self.idle_color)
                    .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                    .finish(),
            )
            .with_width(IDLE_DOT_SIZE)
            .with_height(IDLE_DOT_SIZE)
            .finish(),
            _ => ConstrainedBox::new(
                WarpIcon::Check
                    .to_warpui_icon(ThemeFill::Solid(self.done_check_color))
                    .finish(),
            )
            .with_width(DONE_CHECK_SIZE)
            .with_height(DONE_CHECK_SIZE)
            .finish(),
        }
    }
}

impl Element for DemotingDoneStatusGlyph {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        ctx: &mut LayoutContext,
        app: &AppContext,
    ) -> Vector2F {
        let phase = demote_done_phase(
            WorkspaceAgentPhase::Done,
            Some(self.phase_changed_at),
            Instant::now(),
        );
        let glyph = self.build_glyph(phase);
        let mut slot = ConstrainedBox::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_main_axis_alignment(MainAxisAlignment::Center)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(glyph)
                .finish(),
        )
        .with_width(self.slot_size)
        .with_height(self.slot_size)
        .finish();
        let size = slot.layout(constraint, ctx, app);
        self.inner = Some(slot);
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, ctx: &mut AfterLayoutContext, app: &AppContext) {
        if let Some(inner) = self.inner.as_mut() {
            inner.after_layout(ctx, app);
        }
    }

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        if let Some(inner) = self.inner.as_mut() {
            inner.paint(origin, ctx, app);
        }
        // While still Done, wake at the demotion boundary so layout swaps to Idle.
        if let Some(delay) = duration_until_done_demotion(
            WorkspaceAgentPhase::Done,
            Some(self.phase_changed_at),
            Instant::now(),
        ) {
            ctx.repaint_after(delay);
        }
    }

    fn size(&self) -> Option<Vector2F> {
        self.size
    }

    fn origin(&self) -> Option<Point> {
        self.origin
    }

    fn dispatch_event(
        &mut self,
        event: &DispatchedEvent,
        ctx: &mut EventContext,
        app: &AppContext,
    ) -> bool {
        self.inner
            .as_mut()
            .is_some_and(|inner| inner.dispatch_event(event, ctx, app))
    }
}
