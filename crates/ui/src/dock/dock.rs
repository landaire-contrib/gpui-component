//! The gpui-component appearance for the dock area: the outer frame, the
//! split frames, and one dock's chrome.

use std::{ops::Deref as _, rc::Rc, sync::Arc};

use gpui::{
    AnyElement, App, AppContext as _, Axis, Context, Div, Element, Empty, InteractiveElement as _,
    IntoElement, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Render, Stateful, Style,
    Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_base::dock::{
    DockAreaRenderer, DockContext, DockEvent, DockPlacement, NodeId, PanelState, PanelView,
    TabGroupRenderer, TilesRenderer,
};

use crate::{
    ActiveTheme as _, Side, StyledExt as _,
    dock::{
        DockSkin, SkinShared, invalid_panel::InvalidPanel, panel_handle, tab_panel::TabGroupSkin,
        tiles::TilesSkin,
    },
    resize_handle,
};

/// The height a closed bottom dock keeps, so its tab bar stays clickable.
const CLOSED_BOTTOM_STRIP: Pixels = px(29.);

/// The payload a dock's resize handle drags. It draws nothing: the handle
/// itself is the affordance.
#[derive(Clone)]
struct ResizePanel;

impl Render for ResizePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl DockAreaRenderer for DockSkin {
    fn frame(&self, _: &mut Window, _: &mut App) -> Stateful<Div> {
        div()
            .id("dock-area")
            .relative()
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_row()
    }

    fn center_frame(&self, _: &mut Window, _: &mut App) -> Stateful<Div> {
        div()
            .id("dock-area-center")
            .flex()
            .flex_1()
            .flex_col()
            .overflow_hidden()
    }

    fn split_frame(&self, node: NodeId, _: Axis, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        // A split frame with no size at all collapses: base puts it between
        // a `resizable_panel` and the resizable group, and between
        // `center_frame` and the centre's root split, and neither parent
        // sizes it. `the_centre_and_the_bottom_dock_share_the_column` fails
        // without this. `size_full` is what the old `StackPanel::render`
        // carried; `flex_1` is belt and braces — either alone passes every
        // case I could construct, so this does not depend on which one wins
        // in a given parent.
        div()
            .id(("dock-split-frame", node.as_u64()))
            .size_full()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .bg(cx.theme().tokens.tab_bar)
    }

    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let placement = dock.placement();

        // A closed left or right dock takes no space at all; a closed bottom
        // dock keeps a strip so its tab bar stays clickable.
        let size = match (dock.is_open(), placement) {
            (true, _) => dock.size(),
            (false, DockPlacement::Bottom) => CLOSED_BOTTOM_STRIP,
            (false, _) => px(0.),
        };

        if size <= px(0.) {
            return div().into_any_element();
        }

        div()
            .flex()
            .flex_none()
            .relative()
            .overflow_hidden()
            .map(|this| match placement {
                DockPlacement::Left | DockPlacement::Right => this.h_flex().h_full().w(size),
                DockPlacement::Bottom => this.w_full().h(size),
                // Base never builds a dock for the centre.
                DockPlacement::Center => this,
            })
            .child(content)
            .child(self.render_resize_handle(dock, window, cx))
            .child(DockResizeTracker {
                dock: dock.clone(),
                shared: self.shared().clone(),
            })
            .into_any_element()
    }

    /// The "unknown panel" message the old `InvalidPanel` drew.
    ///
    /// It answers `dump` with the state it was handed, so a layout written by
    /// a build that knows the panel survives a load and save here.
    fn build_placeholder(
        &self,
        state: &PanelState,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<dyn PanelView>> {
        let state = state.clone();
        Some(panel_handle(cx.new(|cx| {
            InvalidPanel::new(state.panel_name.clone(), state, cx)
        })))
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(TabGroupSkin::new(self.shared().clone()))
    }

    fn tiles_renderer(&self) -> Rc<dyn TilesRenderer> {
        Rc::new(TilesSkin::new(self.shared().clone()))
    }
}

impl DockSkin {
    fn render_resize_handle(
        &self,
        dock: &DockContext,
        _: &mut Window,
        _: &mut App,
    ) -> impl IntoElement {
        let placement = dock.placement();
        let shared = self.shared().clone();

        // One id per placement: the docks all render under the same stateful
        // ancestor, so a shared literal would collapse the handles into one
        // GlobalElementId and GPUI would silently share their element state —
        // a press on the left handle then starts the right handle's drag.
        let id = match placement {
            DockPlacement::Left => "resize-handle-left",
            DockPlacement::Right => "resize-handle-right",
            DockPlacement::Bottom => "resize-handle-bottom",
            DockPlacement::Center => "resize-handle-center",
        };

        resize_handle(id, placement.axis())
            .when(placement.is_left(), |this| this.placement(Side::Left))
            .on_drag(ResizePanel, move |info, _, _, cx| {
                cx.stop_propagation();
                shared.resizing_dock().set(Some(placement));
                cx.new(|_| info.deref().clone())
            })
    }
}

/// Turns the window's mouse stream into dock resizing.
///
/// A resize is driven by pointer moves that land anywhere in the window, not
/// only on the handle, so it cannot be expressed as a listener on the handle
/// itself. This element paints nothing and exists for its `paint` hook, which
/// is the only place a window-level mouse listener can be registered — which
/// is why it stays in the skin rather than moving into base: it is a
/// paint-order concern of this appearance.
struct DockResizeTracker {
    dock: DockContext,
    shared: Rc<SkinShared>,
}

impl IntoElement for DockResizeTracker {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DockResizeTracker {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        (window.request_layout(Style::default(), None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        let placement = self.dock.placement();

        window.on_mouse_event({
            let dock = self.dock.clone();
            let shared = self.shared.clone();
            move |event: &MouseMoveEvent, phase, window, cx| {
                if !phase.bubble() || shared.resizing_dock().get() != Some(placement) {
                    return;
                }
                // Dragging a closed dock's handle reopens it, as the old dock
                // did. The live state is read rather than the render-time
                // snapshot in `dock`, which would still say closed for the
                // rest of the frame and toggle it shut again on the next move.
                let open = shared
                    .area()
                    .upgrade()
                    .is_some_and(|area| area.read(cx).is_dock_open(placement));
                if !open {
                    dock.toggle(window, cx);
                }
                dock.resize_to(event.position, window, cx);
            }
        });

        window.on_mouse_event({
            let shared = self.shared.clone();
            move |_: &MouseUpEvent, phase, _, cx| {
                if !phase.bubble() || shared.resizing_dock().get() != Some(placement) {
                    return;
                }
                shared.resizing_dock().set(None);
                // The size lives on the dock, not in the layout tree, so
                // nothing else tells a subscriber to persist it.
                _ = shared
                    .area()
                    .update(cx, |_, cx| cx.emit(DockEvent::LayoutChanged));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gpui::{
        Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size,
    };

    use crate::dock::{
        DockArea, DockLayout, DockPlacement, DockSkin, PaneRef, test_support::MeasuredProbe,
    };

    fn area_with_side_docks(cx: &mut TestAppContext) -> (Entity<DockArea>, &mut VisualTestContext) {
        cx.update(|cx| crate::init(cx));
        let (area, cx) = cx.add_window_view(|window, cx| {
            DockArea::new("test", None, window, cx).with_renderer(DockSkin::new(cx))
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| {
            area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::tabs().panel(MeasuredProbe::new(Rc::default(), cx)),
                    window,
                    cx,
                );
                area.set_dock(
                    DockPlacement::Left,
                    DockLayout::tabs().panel(MeasuredProbe::new(Rc::default(), cx)),
                    window,
                    cx,
                );
                area.set_dock(
                    DockPlacement::Right,
                    DockLayout::tabs().panel(MeasuredProbe::new(Rc::default(), cx)),
                    window,
                    cx,
                );
            });
        });
        cx.run_until_parked();
        (area, cx)
    }

    /// Every dock's handle once shared the literal element id
    /// `"resize-handle"`, so GPUI silently handed them one element state —
    /// including the pending-mouse-down that starts a drag. Pressing the left
    /// handle then let the right dock's drag listener (painted later, so
    /// dispatched first) claim the drag, and one pixel of movement threw the
    /// right dock to nearly the full area width.
    #[gpui::test]
    fn dragging_the_left_handle_resizes_only_the_left_dock(cx: &mut TestAppContext) {
        let (area, cx) = area_with_side_docks(cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // The left dock is 200px wide, so its handle sits at x ∈ [198, 199).
        cx.simulate_mouse_down(
            point(px(198.5), px(300.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        // Past the drag threshold: the drag starts and claims a dock.
        cx.simulate_mouse_move(
            point(px(204.), px(300.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        // The move the claimed dock resizes to.
        cx.simulate_mouse_move(
            point(px(240.), px(300.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            point(px(240.), px(300.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.run_until_parked();

        let (left, right) = cx.update(|_, cx| {
            let area = area.read(cx);
            (
                area.dock_size(DockPlacement::Left),
                area.dock_size(DockPlacement::Right),
            )
        });
        assert_eq!(
            right,
            Some(px(200.)),
            "the right dock must not move when the left handle is dragged"
        );
        assert_eq!(left, Some(px(240.)), "the left dock follows the pointer");
    }

    /// `DockArea::node_bounds` records each center leaf's real on-screen rect,
    /// keyed by the same `NodeId` a caller walks the tree with -- so a host can
    /// paint a spatial overlay (the pane picker) over each pane.
    ///
    /// Regression guard for the `on_prepaint` probe order in
    /// `DockArea::render_node`. The measurement canvas is absolutely positioned
    /// with no inset, so it takes its *static* position; placed after the pane
    /// content it lands below it, and the captured `origin.y` comes back offset
    /// by the pane's own height -- pushing any overlay off the bottom of the
    /// window. The probe must precede the content so it measures the wrapper's
    /// origin. This asserts the two side-by-side leaves are top-anchored and
    /// tiled, which fails when the probe records its static position instead.
    #[gpui::test]
    fn node_bounds_capture_leaf_rects_at_the_docks_top(cx: &mut TestAppContext) {
        cx.update(|cx| crate::init(cx));
        let (area, cx) = cx.add_window_view(|window, cx| {
            DockArea::new("test", None, window, cx).with_renderer(DockSkin::new(cx))
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| {
            area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::h_split()
                        .child(DockLayout::tabs().panel(MeasuredProbe::new(Rc::default(), cx)), None)
                        .child(DockLayout::tabs().panel(MeasuredProbe::new(Rc::default(), cx)), None),
                    window,
                    cx,
                );
            });
        });
        cx.run_until_parked();
        // Force a real paint so the leaves' `on_prepaint` probes fire.
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let (left_id, right_id) = cx.update(|_, cx| {
            let area = area.read(cx);
            let tree = area.layout(DockPlacement::Center).expect("a center tree");
            let PaneRef::Split { children, .. } = tree.root().kind() else {
                panic!("the center root is a horizontal split");
            };
            (children[0].id(), children[1].id())
        });

        let (left, right) = cx.update(|_, cx| {
            let area = area.read(cx);
            (
                area.node_bounds(left_id).expect("left leaf rect captured during paint"),
                area.node_bounds(right_id).expect("right leaf rect captured during paint"),
            )
        });

        // Two leaves side by side: a shared top edge and height, tiled along x.
        assert_eq!(left.origin.y, right.origin.y, "the two leaves share a top edge");
        assert_eq!(left.size.height, right.size.height, "the two leaves are the same height");
        assert!(left.size.height > px(0.), "the leaf has a real height");
        assert!(left.origin.x < right.origin.x, "the left leaf sits left of the right");

        // Top-anchored: the probe recorded the wrapper's origin, not a static
        // position below the content. The regression set `origin.y` to the pane
        // height, pushing the rect (and any overlay) off the bottom of the view.
        assert!(
            left.origin.y < px(1.),
            "the leaf rect starts at the dock's top; got origin.y {:?}",
            left.origin.y,
        );
        assert!(
            left.origin.y + left.size.height <= px(601.),
            "the leaf rect fits inside the 600px window; got origin.y {:?} + height {:?}",
            left.origin.y,
            left.size.height,
        );
    }
}
