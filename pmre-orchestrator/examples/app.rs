//! Live interactive window. Real mouse clicks, wheel scrolling, and resizing drive the same
//! `render_ui` / `handle_event` engine the headless `ui` example exercises. The kit and the
//! orchestrator library stay dependency-free; only this runner uses winit + softbuffer (a
//! pure-CPU presentation surface — our math framebuffer is blitted straight to the window).
//!
//! Run: cargo run -p pmre-orchestrator --example app

use std::num::NonZeroU32;
use std::rc::Rc;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use pmre_kit::{
    ux::{Align, Dim, Edges, Justify, Style, UxNode},
    Rgba,
};
use pmre_orchestrator::{handle_event, render_ui, UiEvent, UiState};

const BG: Rgba = Rgba::new(0.078, 0.086, 0.110, 1.0);
const PANEL: Rgba = Rgba::new(0.118, 0.129, 0.161, 1.0);

fn white() -> Rgba {
    Rgba::rgb8(235, 239, 247)
}
fn muted() -> Rgba {
    Rgba::rgb8(150, 158, 174)
}

const TOG_DARK: u32 = 1;
const BTN_SAVE: u32 = 2;
const BTN_CANCEL: u32 = 3;
const TOG_OPT: u32 = 4;
const LIST: u32 = 10;

fn button(s: &UiState, id: u32, label: &str, base: Rgba) -> UxNode {
    let bg = if s.is_pressed(id) {
        Rgba::rgb8(40, 44, 56)
    } else if s.is_hover(id) {
        Rgba::new(base.r * 1.25, base.g * 1.25, base.b * 1.25, 1.0)
    } else {
        base
    };
    UxNode::boxed(
        Style::row()
            .button(id)
            .w(Dim::Flex(1.0))
            .h(Dim::Px(40.0))
            .radius(8.0)
            .bg(bg)
            .align(Align::Center)
            .justify(Justify::Center),
        vec![UxNode::text(label, 14.0, white())],
    )
}

fn toggle(s: &UiState, id: u32) -> UxNode {
    let on = s.toggle_on(id);
    let track = if on {
        Rgba::rgb8(52, 199, 130)
    } else {
        Rgba::rgb8(70, 74, 90)
    };
    let knob = UxNode::boxed(
        Style::col()
            .w(Dim::Px(22.0))
            .h(Dim::Px(22.0))
            .radius(11.0)
            .bg(white()),
        vec![],
    );
    UxNode::boxed(
        Style::row()
            .toggle(id)
            .w(Dim::Px(52.0))
            .h(Dim::Px(28.0))
            .radius(14.0)
            .bg(track)
            .align(Align::Center)
            .pad(Edges::xy(3.0, 0.0))
            .justify(if on { Justify::End } else { Justify::Start }),
        vec![knob],
    )
}

fn row(i: u32) -> UxNode {
    let shade = if i.is_multiple_of(2) { 30 } else { 36 };
    UxNode::boxed(
        Style::row()
            .h(Dim::Px(40.0))
            .radius(8.0)
            .bg(Rgba::rgb8(shade, shade + 3, shade + 10))
            .align(Align::Center)
            .pad(Edges::xy(12.0, 0.0)),
        vec![UxNode::text(
            format!("ITEM {i:02} - CLICK A TOGGLE, DRAG NOTHING, SCROLL ME"),
            13.0,
            muted(),
        )],
    )
}

fn build(s: &UiState) -> UxNode {
    let spacer = UxNode::boxed(Style::row().w(Dim::Flex(1.0)).h(Dim::Px(1.0)), vec![]);
    let header = UxNode::boxed(
        Style::row()
            .h(Dim::Px(56.0))
            .bg(PANEL)
            .align(Align::Center)
            .pad(Edges::xy(18.0, 0.0))
            .gap(14.0),
        vec![
            UxNode::text("CONTROLS", 18.0, white()),
            spacer,
            UxNode::text("DARK MODE", 13.0, muted()),
            toggle(s, TOG_DARK),
        ],
    );
    let sidebar = UxNode::boxed(
        Style::col()
            .w(Dim::Px(190.0))
            .h(Dim::Flex(1.0))
            .bg(PANEL)
            .pad(Edges::all(16.0))
            .gap(12.0),
        vec![
            UxNode::text("ACTIONS", 12.0, muted()),
            button(s, BTN_SAVE, "SAVE", Rgba::rgb8(48, 110, 210)),
            button(s, BTN_CANCEL, "CANCEL", Rgba::rgb8(70, 74, 90)),
            UxNode::text("OPTION", 12.0, muted()),
            UxNode::boxed(
                Style::row().align(Align::Center).gap(10.0).h(Dim::Px(28.0)),
                vec![toggle(s, TOG_OPT), UxNode::text("ENABLED", 13.0, muted())],
            ),
        ],
    );
    let list = UxNode::boxed(
        Style::col()
            .scroll(LIST)
            .w(Dim::Flex(1.0))
            .h(Dim::Flex(1.0))
            .bg(Rgba::rgb8(24, 26, 33))
            .pad(Edges::all(12.0))
            .gap(8.0),
        (0..20).map(row).collect(),
    );
    let body = UxNode::boxed(
        Style::row().w(Dim::Flex(1.0)).h(Dim::Flex(1.0)),
        vec![sidebar, list],
    );
    UxNode::boxed(
        Style::col().w(Dim::Flex(1.0)).h(Dim::Flex(1.0)).bg(BG),
        vec![header, body],
    )
}

#[derive(Default)]
struct App {
    window: Option<Rc<Window>>,
    context: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    state: UiState,
    cursor: (f32, f32),
}

impl App {
    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn paint(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let (w, h) = (self.state.width.max(1), self.state.height.max(1));
        surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
            .unwrap();
        let build_ref: &dyn Fn(&UiState) -> UxNode = &build;
        let fb = render_ui(build_ref, &self.state, BG);
        let pixels = fb.to_u32(BG);
        let mut buf = surface.buffer_mut().unwrap();
        let n = buf.len().min(pixels.len());
        buf[..n].copy_from_slice(&pixels[..n]);
        window.pre_present_notify();
        buf.present().unwrap();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("pmre — pure-math UI")
            .with_inner_size(LogicalSize::new(960.0, 620.0));
        let window = Rc::new(el.create_window(attrs).expect("create window"));
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        let size = window.inner_size();
        self.state = UiState::new(size.width.max(1), size.height.max(1));
        self.window = Some(window.clone());
        self.context = Some(context);
        self.surface = Some(surface);
        window.request_redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let build_ref: &dyn Fn(&UiState) -> UxNode = &build;
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => {
                handle_event(
                    &mut self.state,
                    build_ref,
                    UiEvent::Resize(sz.width.max(1), sz.height.max(1)),
                );
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                handle_event(
                    &mut self.state,
                    build_ref,
                    UiEvent::PointerMove(self.cursor.0, self.cursor.1),
                );
                self.redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor;
                let ev = if state == ElementState::Pressed {
                    UiEvent::PointerDown(x, y)
                } else {
                    UiEvent::PointerUp(x, y)
                };
                handle_event(&mut self.state, build_ref, ev);
                self.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 48.0,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                let (x, y) = self.cursor;
                handle_event(&mut self.state, build_ref, UiEvent::Wheel(x, y, -dy));
                self.redraw();
            }
            WindowEvent::RedrawRequested => self.paint(),
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run app");
}
