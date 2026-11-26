//! Phase 0 probe: list the compositor's globals and wl_shm formats.
//! Run on the TV with `XDG_RUNTIME_DIR=/tmp/xdg`.
use wayland_client::protocol::{wl_registry, wl_shm};
use wayland_client::{Connection, Dispatch, QueueHandle};

#[derive(Default)]
struct Probe {
    globals: Vec<(String, u32)>,
    formats: Vec<String>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Probe {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            if interface == "wl_shm" {
                registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ());
            }
            state.globals.push((interface, version));
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for Probe {
    fn event(state: &mut Self, _: &wl_shm::WlShm, event: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_shm::Event::Format { format } = event {
            state.formats.push(format!("{format:?}"));
        }
    }
}

fn main() -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("connect: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut probe = Probe::default();
    queue.roundtrip(&mut probe).map_err(|e| format!("roundtrip: {e}"))?;
    queue.roundtrip(&mut probe).map_err(|e| format!("roundtrip: {e}"))?;
    for (i, v) in &probe.globals {
        println!("global {i} v{v}");
    }
    for f in &probe.formats {
        println!("shm {f}");
    }
    Ok(())
}
