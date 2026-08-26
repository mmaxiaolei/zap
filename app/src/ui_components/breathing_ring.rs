//! 呼吸环透明度与零尺寸 ticker Element,用于 Agent 头像进行中状态的周期动画。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use instant::Instant;
use pathfinder_geometry::vector::Vector2F;
use warpui::elements::{Element, Point};
use warpui::event::DispatchedEvent;
use warpui::{
    AfterLayoutContext, AppContext, EventContext, LayoutContext, PaintContext, SizeConstraint,
};

pub(crate) const BREATHING_PERIOD: Duration = Duration::from_millis(1600);
const REPAINT_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub(crate) struct BreathingStateHandle(Arc<Mutex<Instant>>);

impl Default for BreathingStateHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl BreathingStateHandle {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.0.lock().expect("breathing state poisoned").elapsed()
    }
}

pub(crate) fn breathing_opacity(elapsed: Duration, period: Duration) -> u8 {
    let period_secs = period.as_secs_f32().max(f32::EPSILON);
    let turns = elapsed.as_secs_f32() / period_secs;
    let wave = (1.0 - (turns * std::f32::consts::TAU).cos()) * 0.5;
    ((0.4 + 0.6 * wave) * 255.0).round() as u8
}

pub(crate) struct BreathingTicker {
    state: BreathingStateHandle,
    origin: Option<Point>,
    size: Option<Vector2F>,
}

impl BreathingTicker {
    pub(crate) fn new(state: BreathingStateHandle) -> Self {
        Self {
            state,
            origin: None,
            size: None,
        }
    }
}

impl Element for BreathingTicker {
    fn layout(
        &mut self,
        _constraint: SizeConstraint,
        _ctx: &mut LayoutContext,
        _app: &AppContext,
    ) -> Vector2F {
        let size = Vector2F::zero();
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, _ctx: &mut AfterLayoutContext, _app: &AppContext) {}

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, _app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        // 持有 state 跨帧存活; 实际 elapsed 由父级 render 消费。
        let _elapsed = self.state.elapsed();
        ctx.repaint_after(REPAINT_INTERVAL);
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

#[cfg(test)]
#[path = "breathing_ring_tests.rs"]
mod tests;
