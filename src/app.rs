//! The `app` module contains methods and structs to actually run your game mainloop
//! and handle top-level state, as well as forward input events such as keyboard
//! and mouse.
//!
//! If you don't want to use `ggez`'s built in event loop, you can
//! write your own mainloop and check for events on your own.  This is
//! not particularly hard, there's nothing special about the
//! `EventHandler` trait.  It just tries to simplify the process a
//! little.  For examples of how to write your own main loop, see the
//! source code for this and the [`app`][crate::app] module, or the
//! [`eventloop` example](https://github.com/ggez/ggez/blob/master/examples/eventloop.rs).

use std::marker::PhantomData;
use winit::{
    dpi,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
};

use crate::context::{ContextFields, HasMut};
use crate::event::{self, ErrorOrigin, EventHandler};
use crate::graphics::GraphicsContext;
use crate::input::{keyboard::KeyInput, keyboard::KeyboardContext, mouse::MouseContext};
use crate::{GameError, GameResult};

#[cfg(feature = "gamepad")]
use crate::input::gamepad::GamepadContext;
#[cfg(feature = "gamepad")]
pub use crate::input::gamepad::GamepadId;
#[cfg(feature = "gamepad")]
pub use gilrs::{Axis, Button};

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

/// Runs the game's main loop, calling event callbacks on the given state
/// object as events occur.
///
/// It does not try to do any type of framerate limiting.  See the
/// documentation for the [`timer`](../timer/index.html) module for more info.
pub fn run<S, C, E>(ctx: C, event_loop: EventLoop<()>, state: S) -> GameResult
where
    S: EventHandler<C, E> + 'static,
    E: 'static + std::fmt::Debug,
    C: 'static
        + HasMut<ContextFields>
        + HasMut<GraphicsContext>
        + HasMut<KeyboardContext>
        + HasMut<MouseContext>
        + HasMut<GamepadContext>
        + HasMut<crate::timer::TimeContext>,
{
    let mut app = GgezApplicationHandler::new(ctx, state);

    event_loop
        .run_app(&mut app)
        .map_err(GameError::EventLoopError)
}

/// Holds any of the possible state specific [`AppHandler`] structs.
#[derive(Debug)]
enum Application<S, C, E> {
    /// Application is starting up. No window or rendering surface is available yet.
    Starting(AppHandler<S, C, E, state::Starting>),
    /// Application is running. Window and rendering surface are available.
    Running(AppHandler<S, C, E, state::Running>),
    /// Application is suspended. Window and rendering surface are invalidated.
    Suspended(AppHandler<S, C, E, state::Suspended>),
}

impl<S, C, E> From<AppHandler<S, C, E, state::Starting>> for Application<S, C, E> {
    fn from(value: AppHandler<S, C, E, state::Starting>) -> Self {
        Self::Starting(value)
    }
}

impl<S, C, E> From<AppHandler<S, C, E, state::Running>> for Application<S, C, E> {
    fn from(value: AppHandler<S, C, E, state::Running>) -> Self {
        Self::Running(value)
    }
}

impl<S, C, E> From<AppHandler<S, C, E, state::Suspended>> for Application<S, C, E> {
    fn from(value: AppHandler<S, C, E, state::Suspended>) -> Self {
        Self::Suspended(value)
    }
}

/// Internal struct implementing the [`winit::application::ApplicationHandler`] trait.
/// This forwards all winit event callbacks to the state specific [`AppHandler`].
#[derive(Debug)]
pub struct GgezApplicationHandler<S, C, E> {
    app: Option<Application<S, C, E>>,
}

impl<S, C, E> GgezApplicationHandler<S, C, E> {
    fn new(ctx: C, state: S) -> Self {
        let app = Application::Starting(AppHandler {
            ctx,
            state,
            _p: PhantomData,
        });
        Self { app: Some(app) }
    }
}

impl<S, C, E> ApplicationHandler<()> for GgezApplicationHandler<S, C, E>
where
    S: EventHandler<C, E> + 'static,
    E: std::fmt::Debug,
    C: 'static
        + HasMut<ContextFields>
        + HasMut<GraphicsContext>
        + HasMut<KeyboardContext>
        + HasMut<MouseContext>
        + HasMut<GamepadContext>
        + HasMut<crate::timer::TimeContext>,
{
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        // SAFETY: The option is only ever `None` during state transitions.
        match self.app.as_mut().unwrap() {
            Application::Starting(ref mut handler) => handler.new_events(event_loop, cause),
            Application::Running(ref mut handler) => handler.new_events(event_loop, cause),
            Application::Suspended(ref mut handler) => handler.new_events(event_loop, cause),
        };
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // SAFETY: The option is set to `None` during the state transition.
        //         After the transition is complete, its result is put back.
        let next = match self.app.take().unwrap() {
            Application::Starting(handler) => {
                let next = handler.resumed(event_loop);
                Application::from(next)
            }
            Application::Suspended(handler) => {
                let next = handler.resumed(event_loop);
                Application::from(next)
            }
            app => app,
        };

        let _ = self.app.insert(next);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        // SAFETY: The option is set to `None` during the state transition.
        //         After the transition is complete, its result is put back.
        let next = match self.app.take().unwrap() {
            Application::Running(handler) => {
                let next = handler.suspended(event_loop);
                Application::from(next)
            }
            app => app,
        };

        let _ = self.app.insert(next);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // If you are writing your own event loop, make sure to include
        // a call to `event::process_window_event()`.  This updates
        // ggez's internal state however necessary.

        // SAFETY: The option is only ever `None` during state transitions.
        if let Application::Running(ref mut handler) = self.app.as_mut().unwrap() {
            handler.window_event(event_loop, window_id, event);
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        // If you are writing your own event loop, make sure to include
        // a call to `event::process_device_event()`.  This updates
        // ggez's internal state however necessary.

        // SAFETY: The option is only ever `None` during state transitions.
        if let Application::Running(ref mut handler) = self.app.as_mut().unwrap() {
            handler.device_event(event_loop, device_id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // If you are writing your own event loop, make sure you include calls
        // to `TimeContext::tick()`, `KeyboardContext::save_keyboard_state()`,
        // `MouseContext::reset_delta()`, and `MouseContext::save_mouse_state()`.
        //
        // These update ggez's internal state however necessary.
        //
        // Make sure to include calls to `GraphicsContext::begin_frame()` and
        // `GraphicsContext::end_frame()` before/after the drawing routine in
        // your custom event loop.

        // SAFETY: The option is only ever `None` during state transitions.
        if let Application::Running(ref mut handler) = self.app.as_mut().unwrap() {
            handler.about_to_wait(event_loop);
        }
    }
}

/// Marker structs for the application state.
mod state {
    #[derive(Debug)]
    pub struct Starting;
    #[derive(Debug)]
    pub struct Running;
    #[derive(Debug)]
    pub struct Suspended;
}

/// Marker trait for all application state marker.
trait AppState {}
impl AppState for state::Starting {}
impl AppState for state::Running {}
impl AppState for state::Suspended {}

#[derive(Debug)]
struct AppHandler<S, C, E, State: AppState> {
    ctx: C,
    state: S,
    _p: PhantomData<(E, State)>,
}

// AppHandler<AppState>
//   - default implementation
//   - new_events() handles `quit_requested` to stop event loop
impl<S, C, E, State: AppState> AppHandler<S, C, E, State>
where
    S: EventHandler<C, E> + 'static,
    E: std::fmt::Debug,
    C: 'static + HasMut<ContextFields> + HasMut<MouseContext>,
{
    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        if HasMut::<ContextFields>::retrieve_mut(&mut self.ctx).quit_requested {
            let res = self.state.quit_event(&mut self.ctx);
            HasMut::<ContextFields>::retrieve_mut(&mut self.ctx).quit_requested = false;
            if let Ok(false) = res {
                HasMut::<ContextFields>::retrieve_mut(&mut self.ctx).continuing = false;
            } else if self.catch_error(res, event_loop, ErrorOrigin::QuitEvent) {
                event_loop.exit();
            }
        }

        if !HasMut::<ContextFields>::retrieve_mut(&mut self.ctx).continuing {
            event_loop.exit();
            return;
        }

        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn catch_error<T>(
        &mut self,
        event_result: Result<T, E>,
        event_loop: &ActiveEventLoop,
        origin: ErrorOrigin,
    ) -> bool {
        if let Err(e) = event_result {
            error!("Error on EventHandler {origin:?}: {e:?}");
            eprintln!("Error on EventHandler {origin:?}: {e:?}");
            if self.state.on_error(&mut self.ctx, origin, e) {
                event_loop.exit();
                return true;
            }
        }

        false
    }

    fn transition<N: AppState + 'static>(self) -> AppHandler<S, C, E, N> {
        AppHandler {
            ctx: self.ctx,
            state: self.state,
            _p: PhantomData,
        }
    }
}

// AppHandler<state::Starting>
//  - resumed() initializes context, creates window and surface, calls load() ==> state::Running,
impl<S, C, E> AppHandler<S, C, E, state::Starting>
where
    S: EventHandler<C, E> + 'static,
    E: std::fmt::Debug,
    C: 'static + HasMut<ContextFields> + HasMut<MouseContext>,
{
    fn resumed(mut self, event_loop: &ActiveEventLoop) -> AppHandler<S, C, E, state::Running> {
        // TODO create window and surface

        let res = self.state.load(&mut self.ctx);
        let _ = self.catch_error(res, event_loop, ErrorOrigin::Load);

        self.transition()
    }
}

// AppHandler<state::Suspended>
//   - resumed() re-creates window and surface ==> state::Running,
impl<S, C, E> AppHandler<S, C, E, state::Suspended>
where
    S: EventHandler<C, E> + 'static,
    E: std::fmt::Debug,
    C: 'static + HasMut<ContextFields> + HasMut<MouseContext>,
{
    fn resumed(self, _event_loop: &ActiveEventLoop) -> AppHandler<S, C, E, state::Running> {
        // TODO re-create window and surface

        self.transition()
    }
}

// AppHandler<state::Running>
//   - handle all events, call update() and draw(),
//   - suspended() drops window and surface ==> state::Suspended,
impl<S, C, E> AppHandler<S, C, E, state::Running>
where
    S: EventHandler<C, E> + 'static,
    E: std::fmt::Debug,
    C: 'static
        + HasMut<ContextFields>
        + HasMut<MouseContext>
        + HasMut<GraphicsContext>
        + HasMut<crate::input::keyboard::KeyboardContext>
        + HasMut<crate::input::mouse::MouseContext>
        + HasMut<GamepadContext>
        + HasMut<crate::timer::TimeContext>,
{
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        mut window_id: WindowId,
        mut event: WindowEvent,
    ) {
        event::process_window_event(&mut self.ctx, &mut window_id, &mut event);

        match event {
            WindowEvent::Resized(logical_size) => {
                let res = self.state.resize_event(
                    &mut self.ctx,
                    logical_size.width as f32,
                    logical_size.height as f32,
                );
                let _ = self.catch_error(res, event_loop, ErrorOrigin::ResizeEvent);
            }
            WindowEvent::CloseRequested => {
                let res = self.state.quit_event(&mut self.ctx);
                if let Ok(false) = res {
                    HasMut::<ContextFields>::retrieve_mut(&mut self.ctx).continuing = false;
                } else if self.catch_error(res, event_loop, ErrorOrigin::QuitEvent) {
                }
            }
            WindowEvent::Focused(gained) => {
                let res = self.state.focus_event(&mut self.ctx, gained);
                let _ = self.catch_error(res, event_loop, ErrorOrigin::FocusEvent);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                HasMut::<KeyboardContext>::retrieve_mut(&mut self.ctx).active_modifiers =
                    modifiers.state()
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let mods = HasMut::<KeyboardContext>::retrieve_mut(&mut self.ctx).active_modifiers;

                let repeat =
                    HasMut::<KeyboardContext>::retrieve_mut(&mut self.ctx).is_key_repeated();
                let key_state = event.state;
                let input = KeyInput { event, mods };
                let (res, origin) = match key_state {
                    ElementState::Pressed => (
                        self.state.key_down_event(&mut self.ctx, input, repeat),
                        ErrorOrigin::KeyDownEvent,
                    ),
                    ElementState::Released => (
                        self.state.key_up_event(&mut self.ctx, input),
                        ErrorOrigin::KeyUpEvent,
                    ),
                };
                let _ = self.catch_error(res, event_loop, origin);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let gfx = HasMut::<GraphicsContext>::retrieve_mut(&mut self.ctx);
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(pos) => {
                        let scale_factor = gfx.win.window.scale_factor();
                        let dpi::LogicalPosition { x, y } = pos.to_logical::<f32>(scale_factor);
                        (x, y)
                    }
                };
                let res = self.state.mouse_wheel_event(&mut self.ctx, x, y);
                let _ = self.catch_error(res, event_loop, ErrorOrigin::MouseWheelEvent);
            }
            WindowEvent::MouseInput {
                state: element_state,
                button,
                ..
            } => {
                let position = HasMut::<MouseContext>::retrieve_mut(&mut self.ctx).position();
                match element_state {
                    ElementState::Pressed => {
                        let res = self.state.mouse_button_down_event(
                            &mut self.ctx,
                            button,
                            position.x,
                            position.y,
                        );
                        let _ =
                            self.catch_error(res, event_loop, ErrorOrigin::MouseButtonDownEvent);
                    }
                    ElementState::Released => {
                        let res = self.state.mouse_button_up_event(
                            &mut self.ctx,
                            button,
                            position.x,
                            position.y,
                        );
                        let _ = self.catch_error(res, event_loop, ErrorOrigin::MouseButtonUpEvent);
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                let position = HasMut::<MouseContext>::retrieve_mut(&mut self.ctx).position();
                let delta = HasMut::<MouseContext>::retrieve_mut(&mut self.ctx).last_delta();
                let res = self.state.mouse_motion_event(
                    &mut self.ctx,
                    position.x,
                    position.y,
                    delta.x,
                    delta.y,
                );
                let _ = self.catch_error(res, event_loop, ErrorOrigin::MouseMotionEvent);
            }
            WindowEvent::Touch(touch) => {
                let res = self.state.touch_event(
                    &mut self.ctx,
                    touch.phase,
                    touch.location.x,
                    touch.location.y,
                );
                let _ = self.catch_error(res, event_loop, ErrorOrigin::TouchEvent);
            }
            WindowEvent::CursorEntered { device_id: _ } => {
                let res = self.state.mouse_enter_or_leave(&mut self.ctx, true);
                let _ = self.catch_error(res, event_loop, ErrorOrigin::MouseEnterOrLeave);
            }
            WindowEvent::CursorLeft { device_id: _ } => {
                let res = self.state.mouse_enter_or_leave(&mut self.ctx, false);
                let _ = self.catch_error(res, event_loop, ErrorOrigin::MouseEnterOrLeave);
            }
            _x => {
                // trace!("ignoring window event {:?}", x);
            }
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        mut device_id: DeviceId,
        mut event: DeviceEvent,
    ) {
        event::process_device_event(&mut self.ctx, &mut device_id, &mut event);

        if let DeviceEvent::MouseMotion { delta } = event {
            let res = self
                .state
                .raw_mouse_motion_event(&mut self.ctx, delta.0, delta.0);
            let _ = self.catch_error(res, event_loop, ErrorOrigin::RawMouseMotionEvent);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let time = HasMut::<crate::timer::TimeContext>::retrieve_mut(&mut self.ctx);
        time.tick();

        // Handle gamepad events if necessary.
        #[cfg(feature = "gamepad")]
        while let Some(gilrs::Event { id, event, .. }) =
            HasMut::<GamepadContext>::retrieve_mut(&mut self.ctx).next_event()
        {
            match event {
                gilrs::EventType::ButtonPressed(button, _) => {
                    let res =
                        self.state
                            .gamepad_button_down_event(&mut self.ctx, button, GamepadId(id));
                    if self.catch_error(res, event_loop, ErrorOrigin::GamepadButtonDownEvent) {
                        return;
                    };
                }
                gilrs::EventType::ButtonReleased(button, _) => {
                    let res =
                        self.state
                            .gamepad_button_up_event(&mut self.ctx, button, GamepadId(id));
                    if self.catch_error(res, event_loop, ErrorOrigin::GamepadButtonUpEvent) {
                        return;
                    };
                }
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    let res =
                        self.state
                            .gamepad_axis_event(&mut self.ctx, axis, value, GamepadId(id));
                    if self.catch_error(res, event_loop, ErrorOrigin::GamepadAxisEvent) {
                        return;
                    };
                }
                _ => {}
            }
        }

        let res = self.state.update(&mut self.ctx);
        if self.catch_error(res, event_loop, ErrorOrigin::Update) {
            return;
        };

        if let Err(e) = HasMut::<GraphicsContext>::retrieve_mut(&mut self.ctx).begin_frame() {
            error!("Error on GraphicsContext::begin_frame(): {e:?}");
            eprintln!("Error on GraphicsContext::begin_frame(): {e:?}");
            event_loop.exit();
        }

        if let Err(e) = self.state.draw(&mut self.ctx) {
            error!("Error on EventHandler::draw(): {e:?}");
            eprintln!("Error on EventHandler::draw(): {e:?}");
            if self.state.on_error(&mut self.ctx, ErrorOrigin::Draw, e) {
                event_loop.exit();
                return;
            }
        }

        if let Err(e) = HasMut::<GraphicsContext>::retrieve_mut(&mut self.ctx).end_frame() {
            error!("Error on GraphicsContext::end_frame(): {e:?}");
            eprintln!("Error on GraphicsContext::end_frame(): {e:?}");
            event_loop.exit();
        }

        // reset the mouse delta for the next frame
        // necessary because it's calculated cumulatively each cycle
        HasMut::<MouseContext>::retrieve_mut(&mut self.ctx).reset_delta();

        // Copy the state of the keyboard into the KeyboardContext
        // and the mouse into the MouseContext
        HasMut::<KeyboardContext>::retrieve_mut(&mut self.ctx).save_keyboard_state();
        HasMut::<MouseContext>::retrieve_mut(&mut self.ctx).save_mouse_state();
    }

    fn suspended(self, _event_loop: &ActiveEventLoop) -> AppHandler<S, C, E, state::Suspended> {
        // TODO drop window and surface

        self.transition()
    }
}
