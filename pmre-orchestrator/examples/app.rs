//! A live, interactive **todo app** with ZERO external crates. The engine renders pure math
//! into a CPU framebuffer; this runner drives a real OS window directly via raw Win32/GDI FFI
//! — no winit, no softbuffer, no dependencies at all. Type a task and press Enter (or click
//! ADD) to add it, click the box to check it off, x to delete, wheel or drag the bar to scroll.
//!
//! Windows-only (it uses the Win32 API directly). The engine itself renders on every platform
//! with no dependencies — the other examples write images with no window.
//!
//! Run: cargo run -p pmre-orchestrator --example app

#![allow(non_snake_case)]
#![allow(clippy::upper_case_acronyms)] // FFI type aliases mirror the Win32 names

use pmre_kit::{
    ux::{Align, Dim, Edges, Justify, Style, UxNode},
    Rgba,
};
use pmre_orchestrator::UiState;

const BG: Rgba = Rgba::new(0.075, 0.082, 0.106, 1.0);
const NEW_INPUT: u32 = 1;
const ADD: u32 = 2;
const LIST: u32 = 99;
const CHECK_BASE: u32 = 1000;
const DEL_BASE: u32 = 2000;

pub struct Todo {
    pub text: String,
    pub done: bool,
}

fn white() -> Rgba {
    Rgba::rgb8(236, 240, 248)
}
fn muted() -> Rgba {
    Rgba::rgb8(140, 148, 164)
}
fn accent() -> Rgba {
    Rgba::rgb8(86, 150, 252)
}

fn input_field(ui: &UiState) -> UxNode {
    let txt = ui.input_text(NEW_INPUT);
    let focused = ui.is_focused(NEW_INPUT);
    let placeholder = txt.is_empty() && !focused;
    let label = if placeholder {
        "type a task, press Enter"
    } else {
        txt
    };
    let mut children = vec![UxNode::text(
        label,
        16.0,
        if placeholder { muted() } else { white() },
    )];
    if focused {
        children.push(UxNode::boxed(
            Style::col().w(Dim::Px(2.0)).h(Dim::Px(22.0)).bg(white()),
            vec![],
        ));
    }
    UxNode::boxed(
        Style::row()
            .input(NEW_INPUT)
            .w(Dim::Flex(1.0))
            .h(Dim::Px(42.0))
            .align(Align::Center)
            .pad(Edges::xy(12.0, 0.0))
            .radius(8.0)
            .bg(Rgba::rgb8(26, 29, 38))
            .border(
                1.0,
                if focused {
                    accent()
                } else {
                    Rgba::rgb8(48, 52, 66)
                },
            ),
        children,
    )
}

fn add_button(s: &UiState) -> UxNode {
    let base = accent();
    let bg = if s.is_pressed(ADD) {
        Rgba::new(base.r * 0.7, base.g * 0.7, base.b * 0.7, 1.0)
    } else if s.is_hover(ADD) {
        Rgba::new(base.r * 1.2, base.g * 1.2, base.b * 1.2, 1.0)
    } else {
        base
    };
    UxNode::boxed(
        Style::row()
            .button(ADD)
            .w(Dim::Px(72.0))
            .h(Dim::Px(42.0))
            .radius(8.0)
            .bg(bg)
            .align(Align::Center)
            .justify(Justify::Center),
        vec![UxNode::text("ADD", 14.0, white())],
    )
}

fn todo_row(i: usize, todo: &Todo) -> UxNode {
    let check = UxNode::boxed(
        Style::row()
            .button(CHECK_BASE + i as u32)
            .w(Dim::Px(26.0))
            .h(Dim::Px(26.0))
            .radius(6.0)
            .bg(if todo.done {
                Rgba::rgb8(52, 199, 130)
            } else {
                Rgba::rgb8(32, 36, 46)
            })
            .border(1.0, Rgba::rgb8(70, 76, 92)),
        vec![],
    );
    let label_color = if todo.done { muted() } else { white() };
    let spacer = UxNode::boxed(Style::row().w(Dim::Flex(1.0)).h(Dim::Px(1.0)), vec![]);
    let del = UxNode::boxed(
        Style::row()
            .button(DEL_BASE + i as u32)
            .w(Dim::Px(26.0))
            .h(Dim::Px(26.0))
            .radius(6.0)
            .bg(Rgba::rgb8(86, 56, 64))
            .align(Align::Center)
            .justify(Justify::Center),
        vec![UxNode::text("x", 14.0, Rgba::rgb8(240, 200, 205))],
    );
    UxNode::boxed(
        Style::row()
            .h(Dim::Px(44.0))
            .align(Align::Center)
            .gap(10.0)
            .pad(Edges::xy(8.0, 0.0))
            .radius(8.0)
            .bg(Rgba::rgb8(30, 33, 43)),
        vec![
            check,
            UxNode::text(todo.text.clone(), 15.0, label_color),
            spacer,
            del,
        ],
    )
}

fn build(todos: &[Todo], s: &UiState) -> UxNode {
    let header = UxNode::boxed(
        Style::row().h(Dim::Px(34.0)).align(Align::Center),
        vec![UxNode::text("MY TASKS", 22.0, white())],
    );
    let input_row = UxNode::boxed(
        Style::row().h(Dim::Px(42.0)).gap(10.0),
        vec![input_field(s), add_button(s)],
    );
    let rows: Vec<UxNode> = todos
        .iter()
        .enumerate()
        .map(|(i, t)| todo_row(i, t))
        .collect();
    let list = UxNode::boxed(
        Style::col()
            .scroll(LIST)
            .w(Dim::Flex(1.0))
            .h(Dim::Flex(1.0))
            .gap(8.0)
            .pad(Edges::all(8.0))
            .radius(10.0)
            .bg(Rgba::rgb8(20, 22, 30)),
        rows,
    );
    UxNode::boxed(
        Style::col()
            .w(Dim::Flex(1.0))
            .h(Dim::Flex(1.0))
            .pad(Edges::all(16.0))
            .gap(12.0)
            .bg(BG),
        vec![header, input_row, list],
    )
}

fn add_todo(todos: &mut Vec<Todo>, ui: &mut UiState) {
    let text = ui.input_text(NEW_INPUT).trim().to_string();
    if !text.is_empty() {
        todos.push(Todo { text, done: false });
    }
    ui.clear_input(NEW_INPUT);
    ui.focused = Some(NEW_INPUT); // keep typing the next task
}

fn apply_click(todos: &mut Vec<Todo>, ui: &mut UiState, id: u32) {
    if id == ADD {
        add_todo(todos, ui);
    } else if (CHECK_BASE..DEL_BASE).contains(&id) {
        let i = (id - CHECK_BASE) as usize;
        if let Some(t) = todos.get_mut(i) {
            t.done = !t.done;
        }
    } else if id >= DEL_BASE {
        let i = (id - DEL_BASE) as usize;
        if i < todos.len() {
            todos.remove(i);
        }
    }
}

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    println!(
        "The live todo window uses the Win32 API and runs on Windows. The engine itself renders \
         on every platform with zero dependencies — run the headless examples (todo, calc, ui, \
         demo, paths, stroke, gradients, uxi, html) to see it draw to images."
    );
}

/// Direct OS windowing via raw FFI — no winit, no softbuffer, no crates.
#[cfg(windows)]
mod win {
    use super::{add_todo, apply_click, build, Todo, BG};
    use core::ffi::c_void;
    use pmre_kit::ux::UxNode;
    use pmre_orchestrator::{handle_event, render_ui, UiEvent, UiState};
    use std::cell::RefCell;

    type HWND = *mut c_void;
    type HINSTANCE = *mut c_void;
    type HMENU = *mut c_void;
    type HDC = *mut c_void;
    type HICON = *mut c_void;
    type HCURSOR = *mut c_void;
    type HBRUSH = *mut c_void;
    type WPARAM = usize;
    type LPARAM = isize;
    type LRESULT = isize;
    type WndProc = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>;

    #[repr(C)]
    struct WndClassW {
        style: u32,
        proc_: WndProc,
        cls_extra: i32,
        wnd_extra: i32,
        instance: HINSTANCE,
        icon: HICON,
        cursor: HCURSOR,
        background: HBRUSH,
        menu_name: *const u16,
        class_name: *const u16,
    }
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Msg {
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        time: u32,
        pt: Point,
    }
    #[repr(C)]
    struct PaintStruct {
        hdc: HDC,
        erase: i32,
        paint: Rect,
        restore: i32,
        inc_update: i32,
        reserved: [u8; 32],
    }
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }
    #[repr(C)]
    struct BitmapInfo {
        header: BitmapInfoHeader,
        colors: [u32; 1],
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassW(c: *const WndClassW) -> u16;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: HWND,
            menu: HMENU,
            inst: HINSTANCE,
            param: *mut c_void,
        ) -> HWND;
        fn DefWindowProcW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT;
        fn GetMessageW(msg: *mut Msg, hwnd: HWND, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> LRESULT;
        fn PostQuitMessage(code: i32);
        fn InvalidateRect(hwnd: HWND, rect: *const Rect, erase: i32) -> i32;
        fn BeginPaint(hwnd: HWND, ps: *mut PaintStruct) -> HDC;
        fn EndPaint(hwnd: HWND, ps: *const PaintStruct) -> i32;
        fn LoadCursorW(inst: HINSTANCE, name: *const u16) -> HCURSOR;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn StretchDIBits(
            hdc: HDC,
            xd: i32,
            yd: i32,
            wd: i32,
            hd: i32,
            xs: i32,
            ys: i32,
            ws: i32,
            hs: i32,
            bits: *const c_void,
            info: *const BitmapInfo,
            usage: u32,
            rop: u32,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    }

    const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const CW_USEDEFAULT: i32 = 0x8000_0000u32 as i32;
    const CS_HREDRAW: u32 = 0x0002;
    const CS_VREDRAW: u32 = 0x0001;
    const WM_DESTROY: u32 = 0x0002;
    const WM_PAINT: u32 = 0x000F;
    const WM_SIZE: u32 = 0x0005;
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WM_CHAR: u32 = 0x0102;
    const BI_RGB: u32 = 0;
    const DIB_RGB_COLORS: u32 = 0;
    const SRCCOPY: u32 = 0x00CC_0020;
    const IDC_ARROW: usize = 32512;

    struct App {
        width: u32,
        height: u32,
        ui: UiState,
        cursor: (f32, f32),
        todos: Vec<Todo>,
    }

    thread_local! {
        static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    fn lo(l: LPARAM) -> f32 {
        ((l & 0xFFFF) as i16) as f32
    }
    fn hi(l: LPARAM) -> f32 {
        (((l >> 16) & 0xFFFF) as i16) as f32
    }

    /// Feed one event to the engine, then apply any resulting click / submit to the task list.
    fn dispatch(ev: UiEvent) {
        APP.with(|cell| {
            if let Some(app) = cell.borrow_mut().as_mut() {
                {
                    let b = |s: &UiState| build(&app.todos, s);
                    handle_event(&mut app.ui, &b, ev);
                }
                if let Some(id) = app.ui.take_click() {
                    apply_click(&mut app.todos, &mut app.ui, id);
                }
                if app.ui.take_submit().is_some() {
                    add_todo(&mut app.todos, &mut app.ui);
                }
            }
        });
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            WM_SIZE => {
                let w = (lp & 0xFFFF) as u32;
                let h = ((lp >> 16) & 0xFFFF) as u32;
                APP.with(|cell| {
                    if let Some(app) = cell.borrow_mut().as_mut() {
                        app.width = w.max(1);
                        app.height = h.max(1);
                    }
                });
                dispatch(UiEvent::Resize(w.max(1), h.max(1)));
                InvalidateRect(hwnd, std::ptr::null(), 0);
                0
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP => {
                let (x, y) = (lo(lp), hi(lp));
                APP.with(|cell| {
                    if let Some(app) = cell.borrow_mut().as_mut() {
                        app.cursor = (x, y);
                    }
                });
                dispatch(match msg {
                    WM_LBUTTONDOWN => UiEvent::PointerDown(x, y),
                    WM_LBUTTONUP => UiEvent::PointerUp(x, y),
                    _ => UiEvent::PointerMove(x, y),
                });
                InvalidateRect(hwnd, std::ptr::null(), 0);
                0
            }
            WM_MOUSEWHEEL => {
                let delta = (((wp >> 16) & 0xFFFF) as i16) as f32 / 120.0;
                let cursor = APP.with(|cell| {
                    cell.borrow()
                        .as_ref()
                        .map(|a| a.cursor)
                        .unwrap_or((0.0, 0.0))
                });
                dispatch(UiEvent::Wheel(cursor.0, cursor.1, -delta * 48.0));
                InvalidateRect(hwnd, std::ptr::null(), 0);
                0
            }
            WM_CHAR => {
                let code = wp as u32;
                let ev = match code {
                    8 => Some(UiEvent::Backspace),
                    13 => Some(UiEvent::Enter),
                    _ => char::from_u32(code)
                        .filter(|c| !c.is_control())
                        .map(UiEvent::Char),
                };
                if let Some(ev) = ev {
                    dispatch(ev);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                0
            }
            WM_PAINT => {
                let mut ps: PaintStruct = std::mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut ps);
                APP.with(|cell| {
                    if let Some(app) = cell.borrow_mut().as_mut() {
                        let b: &dyn Fn(&UiState) -> UxNode = &|s| build(&app.todos, s);
                        let fb = render_ui(b, &app.ui, BG);
                        let px = fb.to_u32(BG);
                        let (w, h) = (app.width as i32, app.height as i32);
                        let bmi = BitmapInfo {
                            header: BitmapInfoHeader {
                                size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                                width: w,
                                height: -h, // top-down
                                planes: 1,
                                bit_count: 32,
                                compression: BI_RGB,
                                size_image: 0,
                                x_ppm: 0,
                                y_ppm: 0,
                                clr_used: 0,
                                clr_important: 0,
                            },
                            colors: [0],
                        };
                        StretchDIBits(
                            hdc,
                            0,
                            0,
                            w,
                            h,
                            0,
                            0,
                            w,
                            h,
                            px.as_ptr() as *const c_void,
                            &bmi,
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        );
                    }
                });
                EndPaint(hwnd, &ps);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    pub fn run() {
        unsafe {
            let class_name = wide("pmre_window");
            let title = wide("pmre todo - pure math, zero dependencies");
            let hinst = GetModuleHandleW(std::ptr::null());
            let wc = WndClassW {
                style: CS_HREDRAW | CS_VREDRAW,
                proc_: Some(wndproc),
                cls_extra: 0,
                wnd_extra: 0,
                instance: hinst,
                icon: std::ptr::null_mut(),
                cursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16),
                background: std::ptr::null_mut(),
                menu_name: std::ptr::null(),
                class_name: class_name.as_ptr(),
            };
            RegisterClassW(&wc);

            APP.with(|cell| {
                *cell.borrow_mut() = Some(App {
                    width: 420,
                    height: 560,
                    ui: UiState::new(420, 560),
                    cursor: (0.0, 0.0),
                    todos: Vec::new(),
                });
            });

            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                440,
                620,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                std::ptr::null_mut(),
            );
            if hwnd.is_null() {
                eprintln!("CreateWindowExW failed");
                return;
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);

            let mut msg: Msg = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
