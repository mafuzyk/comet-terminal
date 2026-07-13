mod app;
mod config;
mod event;
mod input;
mod renderer;
mod tab;

use std::sync::mpsc;

use winit::event_loop::EventLoop;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let config = config::load();
    let event_loop = EventLoop::new().unwrap();
    let (app_sender, app_receiver) = mpsc::channel();

    let mut app = app::App::new(config, app_sender, app_receiver, event_loop.create_proxy());

    event_loop.run_app(&mut app).unwrap();
}
